//! Bounded, fresh terminology metadata; public operation hot paths never call this inventory.
use crate::error::AppError;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};

pub(super) async fn systems(pool: &PgPool) -> Result<Vec<Value>, AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    type Row = (
        String,
        Option<String>,
        Option<String>,
        bool,
        sqlx::types::Json<Value>,
    );
    let rows:Vec<Row>=sqlx::query_as("SELECT s.url,s.version,s.native_provider,row_number() OVER (PARTITION BY s.url ORDER BY s.updated_at DESC,s.id)=1,CASE WHEN octet_length(COALESCE(s.content->'property','[]')::text)<=65536 AND octet_length(COALESCE(s.content->'filter','[]')::text)<=65536 THEN jsonb_build_object('content',s.content_mode,'hierarchy',s.hierarchy_meaning,'language',COALESCE(s.content->>'language',r.language),'property',s.content->'property','filter',s.content->'filter','simpleRefsetsLoaded',COALESCE(r.simple_refsets_loaded,false),'primaryLinksLoaded',COALESCE(l.primary_links_loaded,false),'semanticTypesLoaded',COALESCE(x.semantic_types_loaded,false),'attributesLoaded',COALESCE(x.attributes_loaded,false),'relationshipsLoaded',COALESCE(x.relationships_loaded,false)) ELSE 'null'::jsonb END FROM code_systems s LEFT JOIN snomed_releases r ON r.code_system_id=s.id LEFT JOIN rxnorm_releases x ON x.code_system_id=s.id LEFT JOIN loinc_releases l ON l.code_system_id=s.id WHERE s.content_mode IS DISTINCT FROM 'supplement' ORDER BY s.url,s.updated_at DESC,s.id LIMIT 10001").fetch_all(&mut *tx).await?;
    if rows.len() > 10000 {
        return Err(AppError::BadRequest(
            "Terminology inventory exceeds 10000 versions".into(),
        ));
    }
    let mut systems: BTreeMap<String, Value> = BTreeMap::new();
    let mut bytes = 0;
    for (url, version, provider, is_default, metadata) in rows {
        let metadata = if url == term_squid_core::ucum::SYSTEM
            && version.as_deref() == Some(term_squid_core::ucum::VERSION)
        {
            // Canonical 2.2 resolves to the bundled grammar, regardless of stored instance metadata.
            json!({"content":"not-present","language":"en","property":[],"filter":[]})
        } else {
            metadata.0
        };
        if metadata.is_null() {
            return Err(AppError::BadRequest(
                "CodeSystem metadata exceeds 64 KiB".into(),
            ));
        }
        bytes += serde_json::to_vec(&metadata)
            .map_err(|e| AppError::Internal(e.into()))?
            .len();
        if bytes > 32 * 1024 * 1024 {
            return Err(AppError::BadRequest(
                "Terminology metadata exceeds 32 MiB".into(),
            ));
        }
        let can_expand = provider.is_some() || metadata["content"] == "complete";
        let hierarchy = can_expand
            && provider.as_deref() != Some("rxnorm-rrf")
            && (metadata["hierarchy"] == "is-a" || provider.as_deref() == Some("snomed-rf2"));
        let system = systems
            .entry(url.clone())
            .or_insert_with(|| json!({"uri":url,"version":[],"subsumption":false}));
        system["subsumption"] = json!(system["subsumption"] == true || hierarchy);
        let mut properties: BTreeSet<String> = metadata["property"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["code"].as_str().map(str::to_owned))
            .collect();
        let mut filters: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        filters.insert(
            "code".into(),
            if provider.is_none() {
                ["=", "exists", "in", "not-in", "regex"]
                    .into_iter()
                    .collect()
            } else {
                ["="].into_iter().collect()
            },
        );
        filters.insert(
            "concept".into(),
            if hierarchy {
                ["=", "is-a", "descendent-of", "is-not-a"]
                    .into_iter()
                    .collect()
            } else {
                ["="].into_iter().collect()
            },
        );
        if provider.is_none() {
            let concept = filters.get_mut("concept").unwrap();
            concept.extend(["exists", "in", "not-in", "regex"]);
            if hierarchy {
                concept.extend(["child-of", "generalizes"]);
            }
        }
        if provider.as_deref() == Some("snomed-rf2") {
            filters.get_mut("concept").unwrap().insert("generalizes");
            if metadata["simpleRefsetsLoaded"] == true {
                filters.get_mut("concept").unwrap().insert("in");
            }
            filters.insert("constraint".into(), ["="].into_iter().collect());
            properties.extend(
                [
                    "inactive",
                    "moduleId",
                    "definitionStatusId",
                    "effectiveTime",
                ]
                .map(str::to_owned),
            );
        } else if provider.as_deref() == Some("loinc-csv") {
            properties.extend(
                [
                    "source-table",
                    "raw-COMPONENT",
                    "raw-PROPERTY",
                    "raw-TIME_ASPCT",
                    "raw-SYSTEM",
                    "raw-SCALE_TYP",
                    "raw-METHOD_TYP",
                    "raw-EXTERNAL_COPYRIGHT_NOTICE",
                    "STATUS",
                    "CLASS",
                    "CLASSTYPE",
                    "ORDER_OBS",
                    "CONSUMER_NAME",
                    "inactive",
                ]
                .map(str::to_owned),
            );
            if metadata["primaryLinksLoaded"] == true {
                properties.extend(
                    term_squid_core::loinc::PrimaryAxis::all().map(|axis| axis.code().to_owned()),
                );
            } else {
                properties.retain(|property| {
                    term_squid_core::loinc::PrimaryAxis::parse(property).is_none()
                });
            }
        } else if provider.as_deref() == Some("rxnorm-rrf") {
            properties.extend(
                [
                    "source-table",
                    "rxnorm-status",
                    "inactive",
                    "preferredTTY",
                    "TTY",
                    "SAB",
                ]
                .map(str::to_owned),
            );
            if metadata["semanticTypesLoaded"] == true {
                properties.extend(["TUI", "STN", "STY"].map(str::to_owned));
            } else {
                properties.retain(|property| !matches!(property.as_str(), "TUI" | "STN" | "STY"));
            }
            if metadata["attributesLoaded"] == true {
                properties.insert("NDC".into());
            } else {
                properties.remove("NDC");
            }
            if metadata["relationshipsLoaded"] == true {
                for property in term_squid_core::rxnorm::RELATION_TYPES
                    .iter()
                    .chain(term_squid_core::rxnorm::RELATION_LABELS)
                {
                    filters
                        .entry((*property).to_owned())
                        .or_default()
                        .extend(["=", "in"]);
                }
            }
        }
        for property in &properties {
            let kind = metadata["property"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["code"] == *property)
                .and_then(|p| p["type"].as_str());
            let operations = if provider.as_deref() == Some("loinc-csv")
                && term_squid_core::loinc::PrimaryAxis::parse(property).is_some()
            {
                ["=", "in", "exists"].into_iter().collect()
            } else if provider.as_deref() == Some("rxnorm-rrf") {
                match property.as_str() {
                    "TTY" | "SAB" | "STY" | "NDC" => ["=", "in"].into_iter().collect(),
                    "source-table" | "rxnorm-status" | "inactive" => ["="].into_iter().collect(),
                    _ => continue,
                }
            } else if provider.as_deref() == Some("snomed-rf2") {
                ["="].into_iter().collect()
            } else if provider.is_none()
                && kind.is_some_and(|t| !matches!(t, "string" | "code" | "boolean" | "integer"))
            {
                ["exists"].into_iter().collect()
            } else if provider.is_none() {
                let mut operations: BTreeSet<_> =
                    ["=", "exists", "in", "not-in"].into_iter().collect();
                if matches!(kind, Some("string" | "code")) {
                    operations.insert("regex");
                }
                operations
            } else {
                ["=", "exists"].into_iter().collect()
            };
            filters.insert(property.clone(), operations);
        }
        if provider.is_none() && url == term_squid_core::snomed::SYSTEM {
            // A finite edition cannot turn a SNOMED refset selector into a
            // literal list merely because it knows the refset concept itself.
            if let Some(concept) = filters.get_mut("concept") {
                concept.remove("in");
            }
            filters.remove("constraint");
        }
        if !can_expand {
            filters.clear();
        }
        let filter: Vec<_> = filters
            .into_iter()
            .map(|(code, op)| json!({"code":code,"op":op}))
            .collect();
        let mut entry = json!({"isDefault":is_default,"compositional":false,"filter":filter,"property":properties});
        if provider.as_deref() == Some("snomed-rf2") {
            let refsets = if metadata["simpleRefsetsLoaded"] == true {
                "Declared complete concept-only simple refsets support concept in, implicit refset/<id> and scalar ECL ^. Active membership may reference inactive concepts; ECL results remain active-only."
            } else {
                "Simple concept refsets are unavailable for this edition."
            };
            entry["extension"] = json!([{"url":"urn:term-squid:terminology:expression-scope","valueString":format!("Bounded active-substrate ECL: scalar focus concepts, wildcard, ancestor/descendant and direct parent/child operators, AND/OR/comma/MINUS and parentheses. Mixed binary operators require parentheses. Limits: 4096 input bytes, 32 parenthesis levels, 128 aggregate AST nodes and 16 aggregate hierarchy leaves including ordinary filters. {refsets} Global refset enumeration, nonconcept refsets, selector/field projections, refinements, history and hierarchy operators on nested sets/wildcards are unavailable.")}]);
        }
        if provider.as_deref() == Some("loinc-csv") {
            let primary_links = if metadata["primaryLinksLoaded"] == true {
                "loaded; the six coded primary axes support equality, with bounded in/exists extensions"
            } else {
                "unavailable"
            };
            entry["extension"] = json!([{"url":"urn:term-squid:terminology:content-scope","valueString":format!("Primary Loinc.csv terms only; expansion requires explicit concepts or source-table=Loinc.csv. LoincPartLink_Primary.csv: {primary_links}. Part identifiers are property literals, not an independently loaded Part registry. Part lookup, accessory/all-code enumeration and hierarchy remain unavailable.")}]);
        }
        if provider.as_deref() == Some("rxnorm-rrf") {
            let semantic_types = if metadata["semanticTypesLoaded"] == true {
                "loaded; STY filter defaults to TUI, with TUI/STN/STY qualifiers"
            } else {
                "unavailable"
            };
            let attributes = if metadata["attributesLoaded"] == true {
                "loaded; exact normalized RXNORM NDC attributes only"
            } else {
                "unavailable"
            };
            let relationships = if metadata["relationshipsLoaded"] == true {
                "loaded; direct R4 REL/RELA filters select endpoint 1 anchored at CUI/AUI endpoint 2; all sources with blank/N suppression; no transitive inference"
            } else {
                "unavailable"
            };
            entry["extension"] = json!([{"url":"urn:term-squid:terminology:content-scope","valueString":format!("Current active normalized RXNCONSO concepts only; expansion requires explicit concepts or active scope. Historical existence, subsumption and mappings are unavailable. RXNSTY semantic types: {semantic_types}. RXNSAT attributes: {attributes}. RXNREL relationships: {relationships}.")}]);
        }
        if let Some(version) = version {
            entry["code"] = version.into();
        }
        if let Some(language) = metadata["language"].as_str() {
            entry["language"] = json!([language]);
        }
        system["version"].as_array_mut().unwrap().push(entry);
    }
    tx.commit().await?;
    let ucum = systems
        .entry(term_squid_core::ucum::SYSTEM.into())
        .or_insert_with(
            || json!({"uri":term_squid_core::ucum::SYSTEM,"subsumption":false,"version":[]}),
        );
    let versions = ucum["version"].as_array_mut().unwrap();
    for version in versions.iter_mut() {
        version["isDefault"] = false.into();
    }
    if let Some(version) = versions
        .iter_mut()
        .find(|v| v["code"] == term_squid_core::ucum::VERSION)
    {
        version["isDefault"] = true.into();
        version["compositional"] = true.into();
    } else {
        versions.insert(
            0,
            json!({"code":term_squid_core::ucum::VERSION,"isDefault":true,"compositional":true}),
        );
    }
    let mut inventory = Vec::new();
    for mut system in systems.into_values() {
        let versions = system["version"].as_array_mut().expect("Version inventory");
        if versions.len() > 1 {
            if let Some(position) = versions.iter().position(|v| v.get("code").is_none()) {
                let unversioned = versions.remove(position);
                inventory.push(json!({"uri":system["uri"],"subsumption":system["subsumption"],"version":[unversioned]}));
            }
        }
        inventory.push(system);
    }
    Ok(inventory)
}
