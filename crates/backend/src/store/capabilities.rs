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
    let rows:Vec<Row>=sqlx::query_as("SELECT s.url,s.version,s.native_provider,row_number() OVER (PARTITION BY s.url ORDER BY s.updated_at DESC,s.id)=1,CASE WHEN octet_length(COALESCE(s.content->'property','[]')::text)<=65536 AND octet_length(COALESCE(s.content->'filter','[]')::text)<=65536 THEN jsonb_build_object('content',s.content_mode,'hierarchy',s.hierarchy_meaning,'language',COALESCE(s.content->>'language',r.language),'property',s.content->'property','filter',s.content->'filter') ELSE 'null'::jsonb END FROM code_systems s LEFT JOIN snomed_releases r ON r.code_system_id=s.id WHERE s.content_mode IS DISTINCT FROM 'supplement' ORDER BY s.url,s.updated_at DESC,s.id LIMIT 10001").fetch_all(&mut *tx).await?;
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
        }
        for property in &properties {
            let kind = metadata["property"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["code"] == *property)
                .and_then(|p| p["type"].as_str());
            let operations = if provider.as_deref() == Some("snomed-rf2") {
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
        if !can_expand {
            filters.clear();
        }
        let filter: Vec<_> = filters
            .into_iter()
            .map(|(code, op)| json!({"code":code,"op":op}))
            .collect();
        let mut entry = json!({"isDefault":is_default,"compositional":false,"filter":filter,"property":properties});
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
