//! Transactional ingestion shared by the CLI and standalone administration.
// Dynamic SQL interpolates only closed table/fragment inventories; external values are bound.
pub mod package;
mod terminology;
mod upload;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
pub use terminology::import_concepts;
pub use upload::{inspect_upload, InspectedUpload, UploadFormat};
use uuid::Uuid;

pub const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CONCEPTS: usize = 50_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inspection {
    pub resource_type: String,
    pub url: String,
    pub version: Option<String>,
    pub records: usize,
    pub record_kind: String,
}

/// Exact upload format: one UTF-8 R4 terminology resource, never a Bundle/archive.
pub fn inspect(content: &Value) -> Result<Inspection> {
    inspect_with_limit(content, MAX_CONCEPTS, true)
}

/// CLI resource commands keep the engine's established 100000-concept ceiling.
pub fn inspect_cli(content: &Value) -> Result<Inspection> {
    inspect_with_limit(content, 100_000, false)
}
fn inspect_with_limit(content: &Value, concept_limit: usize, r4_only: bool) -> Result<Inspection> {
    let kind = content["resourceType"]
        .as_str()
        .context("Missing resourceType")?;
    ensure!(
        matches!(kind, "CodeSystem" | "ValueSet" | "ConceptMap"),
        "Expected one CodeSystem, ValueSet or ConceptMap (Bundles and archives are unsupported)"
    );
    let url = content["url"].as_str().context("Missing canonical url")?;
    ensure!(
        !url.is_empty() && url.len() <= 512 && !url.contains('|') && url.contains(':'),
        "Invalid canonical url"
    );
    let version = content
        .get("version")
        .map(|v| v.as_str().context("version must be a string"))
        .transpose()?;
    ensure!(
        version.is_none_or(|v| !v.is_empty() && v.len() <= 100 && !v.contains('|')),
        "Invalid business version"
    );
    ensure!(
        matches!(
            content["status"].as_str(),
            Some("draft" | "active" | "retired" | "unknown")
        ),
        "Invalid or missing status"
    );
    for key in ["name", "title", "description", "publisher"] {
        ensure!(
            content.get(key).is_none_or(Value::is_string),
            "Metadata must be strings"
        );
    }
    ensure!(
        content["name"].as_str().is_none_or(|s| s.len() <= 255),
        "Name too long"
    );
    ensure!(
        !r4_only || content["fhirVersion"].as_str().is_none_or(|v| v == "4.0.1"),
        "Only the verified R4 adapter is supported for uploads"
    );
    let mut records = 1;
    let mut record_kind = "resource";
    if kind == "CodeSystem" {
        ensure!(
            matches!(
                content["content"].as_str(),
                Some("complete" | "fragment" | "not-present" | "example" | "supplement")
            ),
            "Invalid or missing CodeSystem content"
        );
        if content["content"] == "supplement" {
            term_squid_core::supplements::target(content)?;
        }
        let definition: term_squid_core::CodeSystem =
            serde_json::from_value(content.clone()).context("Invalid CodeSystem")?;
        let prepared = if r4_only {
            term_squid_core::PreparedSystem::prepare_for_r4_source(definition)?
        } else {
            term_squid_core::PreparedSystem::prepare_for_import(definition)?
        };
        records = prepared.concepts.len();
        record_kind = "concepts";
        ensure!(records <= concept_limit, "Concept limit exceeded");
        ensure!(
            !r4_only
                || prepared
                    .concepts
                    .keys()
                    .all(|code| code.chars().count() <= 255),
            "Concept codes exceed the 255-character projection limit"
        );
    }
    if r4_only && kind == "ValueSet" {
        if let Some(compose) = content.get("compose") {
            let compose: term_squid_core::Compose =
                serde_json::from_value(compose.clone()).context("Invalid ValueSet compose")?;
            records = compose
                .include
                .iter()
                .chain(&compose.exclude)
                .map(|selection| {
                    1 + selection.concept.len() + selection.filter.len() + selection.value_set.len()
                })
                .sum();
            ensure!(
                records <= MAX_CONCEPTS,
                "ValueSet definition entry limit exceeded"
            );
            record_kind = "definition entries";
        }
    }
    if r4_only && kind == "ConceptMap" {
        if let Some(groups) = content.get("group") {
            records = 0;
            for group in groups
                .as_array()
                .context("ConceptMap group must be an array")?
            {
                ensure!(group.is_object(), "ConceptMap group must be an object");
                records += 1;
                if let Some(elements) = group.get("element") {
                    for element in elements
                        .as_array()
                        .context("ConceptMap element must be an array")?
                    {
                        ensure!(
                            element["code"].as_str().is_some_and(|s| !s.is_empty()),
                            "ConceptMap element code is required"
                        );
                        records += 1;
                        if let Some(targets) = element.get("target") {
                            let targets = targets
                                .as_array()
                                .context("ConceptMap target must be an array")?;
                            ensure!(
                                targets.iter().all(Value::is_object),
                                "ConceptMap targets must be objects"
                            );
                            records += targets.len();
                        }
                    }
                }
            }
            ensure!(
                records <= MAX_CONCEPTS,
                "ConceptMap definition entry limit exceeded"
            );
            record_kind = "definition entries";
        }
    }
    if kind == "ConceptMap" {
        // CLI and protected uploads share the same R4 modifier safety boundary.
        term_squid_core::PreparedConceptMap::prepare(content)?;
    }
    Ok(Inspection {
        resource_type: kind.into(),
        url: url.into(),
        version: version.map(String::from),
        records,
        record_kind: record_kind.into(),
    })
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub id: Uuid,
    pub unchanged: bool,
}

/// Serialize canonical/business-version admission across HTTP and CLI writers, including NULL versions.
pub async fn lock_identity(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    url: &str,
    version: Option<&str>,
) -> Result<()> {
    let table = match kind {
        "CodeSystem" => "code_systems",
        "ValueSet" => "value_sets",
        "ConceptMap" => "concept_maps",
        _ => anyhow::bail!("Unsupported resource"),
    };
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!("{table}|{url}|{}", version.unwrap_or("")))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// The caller owns publication: jobs commit this result and their success row together.
/// Same canonical/version is immutable; identical content is idempotent.
pub async fn import_resource(
    tx: &mut Transaction<'_, Postgres>,
    content: &Value,
    meta: &Inspection,
) -> Result<ImportResult> {
    if meta.resource_type == "ConceptMap" {
        // Do not trust a retained Inspection after its JSON payload was changed.
        term_squid_core::PreparedConceptMap::prepare(content)?;
    }
    let table = match meta.resource_type.as_str() {
        "CodeSystem" => "code_systems",
        "ValueSet" => "value_sets",
        "ConceptMap" => "concept_maps",
        _ => anyhow::bail!("Unsupported resource"),
    };
    lock_identity(tx, &meta.resource_type, &meta.url, meta.version.as_deref()).await?;
    let existing: Option<(Uuid, sqlx::types::Json<Value>)> = sqlx::query_as(sqlx::AssertSqlSafe(
        format!("SELECT id,content FROM {table} WHERE url=$1 AND version IS NOT DISTINCT FROM $2"),
    ))
    .bind(&meta.url)
    .bind(&meta.version)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((id, previous)) = existing {
        ensure!(
            previous.0 == *content,
            "Immutable canonical/business-version conflict"
        );
        return Ok(ImportResult {
            id,
            unchanged: true,
        });
    }
    let id = Uuid::new_v4();
    let sql = format!("INSERT INTO {table} (id,url,version,status,name,title,fhir_version,content) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)");
    sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(id)
        .bind(&meta.url)
        .bind(&meta.version)
        .bind(content["status"].as_str())
        .bind(content["name"].as_str())
        .bind(content["title"].as_str())
        .bind(content["fhirVersion"].as_str())
        .bind(sqlx::types::Json(content))
        .execute(&mut **tx)
        .await?;
    if meta.resource_type == "CodeSystem" {
        import_concepts(tx, id, content).await?;
    }
    if meta.resource_type == "ConceptMap" {
        sqlx::query("UPDATE concept_maps SET source_uri=$2,target_uri=$3 WHERE id=$1")
            .bind(id)
            .bind(
                content["sourceUri"]
                    .as_str()
                    .or(content["sourceCanonical"].as_str()),
            )
            .bind(
                content["targetUri"]
                    .as_str()
                    .or(content["targetCanonical"].as_str()),
            )
            .execute(&mut **tx)
            .await?;
    }
    Ok(ImportResult {
        id,
        unchanged: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upload_scope_and_identity_are_explicit() {
        for kind in ["Bundle", "Patient", "Parameters"] {
            assert!(inspect(&serde_json::json!({"resourceType":kind})).is_err());
        }
        let mut r = serde_json::json!({"resourceType":"CodeSystem","url":"urn:test","version":"1","status":"active","content":"complete","concept":[{"code":"a"}]});
        assert_eq!(inspect(&r).unwrap().records, 1);
        r["url"] = "urn:test|1".into();
        assert!(inspect(&r).is_err());
    }
}

#[cfg(test)]
mod definition_tests {
    use super::*;
    #[test]
    fn r4_inspection_rejects_mistyped_properties_before_publication() {
        let mut resource = serde_json::json!({"resourceType":"CodeSystem","url":"urn:flag-types","status":"active","content":"fragment","property":[{"code":"grouping","uri":"http://hl7.org/fhir/concept-properties#notSelectable","type":"boolean"}],"concept":[{"code":"A","property":[{"code":"grouping","valueCode":"true"}]}]});
        assert!(inspect(&resource).is_err());
        // The legacy CLI format remains unchanged; readers still reject unsafe flag interpretation.
        assert!(inspect_cli(&resource).is_ok());
        resource["concept"][0]["property"][0] =
            serde_json::json!({"code":"grouping","valueBoolean":true});
        assert_eq!(inspect(&resource).unwrap().records, 1);
        for code in [String::new(), "x".repeat(256)] {
            resource["concept"][0]["code"] = code.into();
            assert!(inspect(&resource).is_err());
        }
        resource["concept"][0]["code"] = "界".repeat(255).into();
        assert_eq!(inspect(&resource).unwrap().records, 1);
    }
    #[test]
    fn definition_structure_and_entry_budgets_are_explicit() {
        let mut resource = serde_json::json!({"resourceType":"ValueSet","url":"urn:bounded","status":"active","compose":{"include":[{"system":"urn:codes","concept":[{"code":"a"}]}]}});
        assert_eq!(inspect(&resource).unwrap().records, 2);
        resource["compose"]["include"][0]["concept"] = Value::Array(
            (0..MAX_CONCEPTS)
                .map(|n| serde_json::json!({"code":n.to_string()}))
                .collect(),
        );
        assert!(inspect(&resource).is_err());
        resource["compose"] = serde_json::json!({"include":{}});
        assert!(inspect(&resource).is_err());
        let map = serde_json::json!({"resourceType":"ConceptMap","url":"urn:map","status":"active","group":[{"element":[{"code":"a","target":[{"code":"b","equivalence":"equivalent"}]}]}]});
        assert_eq!(inspect(&map).unwrap().records, 3);
        let mut invalid = map;
        invalid["group"][0]["element"] = true.into();
        assert!(inspect(&invalid).is_err());
    }
}

#[cfg(test)]
mod package_tests;

#[cfg(test)]
mod translation_modifier_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_ingestion_mode_requires_the_explicit_r4_mapping_modifier() {
        let map = json!({"resourceType":"ConceptMap","url":"urn:safety:map","status":"active","group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"equivalent"}]}]}]});
        for inspect in [super::inspect, super::inspect_cli] {
            assert!(inspect(&map).is_ok());
            let mut missing = map.clone();
            missing["group"][0]["element"][0]["target"][0]
                .as_object_mut()
                .unwrap()
                .remove("equivalence");
            assert!(inspect(&missing).is_err());
            for value in [
                Value::Null,
                json!(true),
                json!(1),
                json!([]),
                json!({}),
                json!(""),
                json!("invalid"),
            ] {
                let mut malformed = map.clone();
                malformed["group"][0]["element"][0]["target"][0]["equivalence"] = value;
                assert!(inspect(&malformed).is_err(), "{malformed}");
            }
            for equivalence in [
                "relatedto",
                "equivalent",
                "equal",
                "wider",
                "subsumes",
                "narrower",
                "specializes",
                "inexact",
                "unmatched",
                "disjoint",
            ] {
                let mut valid = map.clone();
                valid["group"][0]["element"][0]["target"][0]["equivalence"] = equivalence.into();
                if matches!(equivalence, "unmatched" | "disjoint") {
                    valid["group"][0]["element"][0]["target"][0]
                        .as_object_mut()
                        .unwrap()
                        .remove("code");
                }
                assert!(inspect(&valid).is_ok(), "{valid}");
            }
        }
    }
}
