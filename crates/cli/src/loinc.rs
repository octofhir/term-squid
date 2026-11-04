//! Atomic loading of the attested LOINC main table; accessory files remain separate.
mod reader;
#[cfg(test)]
mod tests;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub version: String,
    pub complete_table: bool,
    pub expected_concepts: u64,
    pub source: Source,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStats {
    pub sha256: String,
    pub rows: u64,
    pub bytes: u64,
    pub file: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub code_system_id: Option<Uuid>,
    pub version: String,
    pub fingerprint: String,
    pub dry_run: bool,
    pub unchanged: bool,
    pub source: SourceStats,
}

fn validate_version(version: &str) -> Result<()> {
    let (major, minor) = version
        .split_once('.')
        .context("LOINC version must be a numeric major.minor business version")?;
    ensure!(
        !major.is_empty()
            && !minor.is_empty()
            && version.len() <= 16
            && major.bytes().all(|b| b.is_ascii_digit())
            && minor.bytes().all(|b| b.is_ascii_digit()),
        "LOINC version must be a numeric major.minor business version"
    );
    Ok(())
}

/// No network or archive extraction. Source authenticity/completeness is operator-attested.
pub async fn import_manifest(pool: &PgPool, path: &Path, dry_run: bool) -> Result<ImportResult> {
    ensure!(
        std::fs::metadata(path)?.is_file(),
        "LOINC manifest must be a regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65_537)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65_536, "LOINC manifest exceeds 64 KiB");
    let manifest: Manifest = serde_json::from_slice(&bytes).context("Invalid LOINC manifest")?;
    ensure!(
        manifest.format == "loinc-csv-v1" && manifest.complete_table,
        "An explicitly attested complete Loinc.csv main table is required"
    );
    validate_version(&manifest.version)?;
    ensure!(
        (1..=reader::MAX_ROWS).contains(&manifest.expected_concepts),
        "Expected LOINC concept count must be between 1 and 2000000"
    );
    ensure!(
        manifest.source.sha256.len() == 64
            && manifest
                .source
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "Expected lowercase source SHA-256"
    );
    ensure!(
        manifest.source.path.file_name().and_then(|n| n.to_str()) == Some("Loinc.csv"),
        "Only the official main-table filename Loinc.csv is supported"
    );
    let identity = json!({
        "format":manifest.format,"projection":"primary-table-v1","version":manifest.version,
        "sourceSha256":manifest.source.sha256,"expectedConcepts":manifest.expected_concepts
    });
    let fingerprint = hex::encode(Sha256::digest(serde_json::to_vec(&identity)?));
    let source_path = if manifest.source.path.is_absolute() {
        manifest.source.path.clone()
    } else {
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&manifest.source.path)
    };
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='15min'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    let source = reader::copy_source(&mut tx, manifest.source.clone(), source_path).await?;
    ensure!(
        source.rows == manifest.expected_concepts,
        "LOINC row count does not match the attested expectedConcepts"
    );
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('term-squid/loinc:'||current_schema()||':'||$1,0))")
        .bind(&manifest.version).execute(&mut *tx).await?;
    let existing: Option<(Uuid, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT c.id,c.native_provider,r.fingerprint FROM code_systems c LEFT JOIN loinc_releases r ON r.code_system_id=c.id WHERE c.url='http://loinc.org' AND c.version=$1")
        .bind(&manifest.version).fetch_optional(&mut *tx).await?;
    if let Some((id, provider, stored)) = existing {
        ensure!(
            provider.as_deref() == Some("loinc-csv")
                && stored.as_deref() == Some(fingerprint.as_str()),
            "Conflicting immutable LOINC release already exists; nothing replaced"
        );
        tx.rollback().await?;
        return Ok(ImportResult {
            code_system_id: Some(id),
            version: manifest.version,
            fingerprint,
            dry_run,
            unchanged: true,
            source,
        });
    }
    if dry_run {
        tx.rollback().await?;
        return Ok(ImportResult {
            code_system_id: None,
            version: manifest.version,
            fingerprint,
            dry_run,
            unchanged: false,
            source,
        });
    }
    let id = Uuid::new_v4();
    let metadata = metadata(id, &manifest.version);
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider,fhir_version,updated_at) VALUES($1,'http://loinc.org',$2,'active','LOINC',$3,'loinc-csv','R4',clock_timestamp())")
        .bind(id).bind(&manifest.version).bind(sqlx::types::Json(metadata)).execute(&mut *tx).await?;
    // The source is retained in PostgreSQL, never in a release-sized Rust map/resource.
    sqlx::query("INSERT INTO loinc_terms(code_system_id,code,fields) SELECT $1,code,fields FROM loinc_input")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,definition,properties) SELECT $1,code,COALESCE(NULLIF(fields->>'LONG_COMMON_NAME',''),NULLIF(fields->>'SHORTNAME','')),NULLIF(fields->>'DefinitionDescription',''), jsonb_build_array(jsonb_build_object('code','source-table','valueString','Loinc.csv'),jsonb_build_object('code','inactive','valueBoolean',fields->>'STATUS'='DEPRECATED')) || COALESCE((SELECT jsonb_agg(jsonb_build_object('code','raw-'||key,'valueString',value)) FROM jsonb_each_text(fields) WHERE key=ANY(ARRAY['COMPONENT','PROPERTY','TIME_ASPCT','SYSTEM','SCALE_TYP','METHOD_TYP','EXTERNAL_COPYRIGHT_NOTICE']) AND value<>''),'[]'::jsonb) || COALESCE((SELECT jsonb_agg(jsonb_build_object('code',key,'valueString',value)) FROM jsonb_each_text(fields) WHERE key=ANY(ARRAY['STATUS','CLASS','CLASSTYPE','ORDER_OBS','CONSUMER_NAME']) AND value<>''),'[]'::jsonb) FROM loinc_input")
        .bind(id).execute(&mut *tx).await?;
    let provenance = json!({
        "identity":identity,"source":source,"completeMainTableAttestation":true,
        "scope":"Numeric primary Loinc.csv terms only; no Parts, Answers, Answer Lists, hierarchy, Groups, maps or translations",
        "columns":"All source columns preserved verbatim in loinc_terms.fields; raw axis labels are not Part identifiers"
    });
    sqlx::query("INSERT INTO loinc_releases(code_system_id,version,fingerprint,provenance) VALUES($1,$2,$3,$4)")
        .bind(id).bind(&manifest.version).bind(&fingerprint).bind(sqlx::types::Json(provenance)).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(ImportResult {
        code_system_id: Some(id),
        version: manifest.version,
        fingerprint,
        dry_run,
        unchanged: false,
        source,
    })
}

fn metadata(id: Uuid, version: &str) -> Value {
    let mut properties = vec![
        json!({"code":"source-table","uri":"urn:term-squid:loinc:source-table","type":"string","description":"Imported source scope; Loinc.csv excludes accessory Parts and Answer String IDs"}),
        json!({"code":"inactive","uri":"http://hl7.org/fhir/concept-properties#inactive","type":"boolean","description":"True only for LOINC STATUS=DEPRECATED"}),
    ];
    for column in [
        "COMPONENT",
        "PROPERTY",
        "TIME_ASPCT",
        "SYSTEM",
        "SCALE_TYP",
        "METHOD_TYP",
        "EXTERNAL_COPYRIGHT_NOTICE",
    ] {
        properties.push(json!({"code":format!("raw-{column}"),"uri":format!("urn:term-squid:loinc:column:{column}"),"type":"string","description":"Verbatim primary-table value; not an LP Part identifier or relationship"}));
    }
    for column in ["STATUS", "CLASS", "CLASSTYPE", "ORDER_OBS", "CONSUMER_NAME"] {
        properties.push(json!({"code":column,"uri":format!("http://loinc.org/property/{column}"),"type":"string"}));
    }
    json!({
        "resourceType":"CodeSystem","id":id,"url":"http://loinc.org","version":version,
        "status":"active","name":"LOINC","content":"not-present","caseSensitive":false,
        "publisher":"Regenstrief Institute, Inc.",
        "copyright":"This content LOINC® is copyright © 1995 Regenstrief Institute, Inc. and the LOINC Committee, and available at no cost under the license at http://loinc.org/terms-of-use",
        "description":"Native primary-table projection only. Parts, Answer String IDs and accessory semantics are not loaded.",
        "property":properties
    })
}
