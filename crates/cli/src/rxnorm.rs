//! Atomic full-monthly RRF ingestion with an explicitly partial current-active projection.
mod auxiliary;
mod reader;
mod relationships;
#[cfg(test)]
mod tests;

use anyhow::{ensure, Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use term_squid_core::rxnorm::{SOURCE_TABLE, SYSTEM};
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
    pub full_monthly: bool,
    pub expected_rows: u64,
    pub expected_concepts: u64,
    pub source: Source,
    pub semantic_types: Option<auxiliary::AuxiliarySource>,
    pub attributes: Option<auxiliary::AuxiliarySource>,
    pub relationships: Option<auxiliary::AuxiliarySource>,
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
    pub concepts: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auxiliary_sources: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_source: Option<Value>,
}

pub async fn import_manifest(pool: &PgPool, path: &Path, dry_run: bool) -> Result<ImportResult> {
    ensure!(
        std::fs::metadata(path)?.is_file(),
        "RxNorm manifest must be a regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65_537)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65_536, "RxNorm manifest exceeds 64 KiB");
    let manifest: Manifest = serde_json::from_slice(&bytes).context("Invalid RxNorm manifest")?;
    ensure!(manifest.format == "rxnorm-rrf-v1" && manifest.full_monthly, "An explicitly attested full monthly RXNCONSO.RRF source is required; Prescribable/weekly subsets are unsupported");
    term_squid_core::rxnorm::validate_version(&manifest.version)?;
    let release = NaiveDate::parse_from_str(&manifest.version, "%m%d%Y")
        .context("Invalid RxNorm release date")?;
    ensure!(
        (1..=reader::MAX_ROWS).contains(&manifest.expected_rows),
        "Expected RxNorm row count must be between 1 and 20000000"
    );
    ensure!(
        (1..=manifest.expected_rows).contains(&manifest.expected_concepts),
        "Expected active RxNorm concept count must be positive and not exceed raw rows"
    );
    ensure!(
        manifest.source.sha256.len() == 64
            && manifest
                .source
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "Expected lowercase source SHA-256"
    );
    ensure!(
        manifest
            .source
            .path
            .file_name()
            .and_then(|name| name.to_str())
            == Some(SOURCE_TABLE),
        "Only the official RXNCONSO.RRF source filename is supported"
    );
    for (kind, source) in [
        (
            auxiliary::Kind::SemanticTypes,
            manifest.semantic_types.as_ref(),
        ),
        (auxiliary::Kind::Attributes, manifest.attributes.as_ref()),
    ] {
        if let Some(source) = source {
            kind.validate(source, &manifest.version)?;
        }
    }
    if let Some(source) = manifest.relationships.as_ref() {
        relationships::validate(source, &manifest.version)?;
    }
    let mut identity = json!({"format":manifest.format,"projection":"current-active-v1","version":manifest.version,
        "fullMonthly":true,"sourceSha256":manifest.source.sha256,"expectedRows":manifest.expected_rows,"expectedConcepts":manifest.expected_concepts});
    // Do not add null/default keys to the already published CONSO-only identity.
    if manifest.semantic_types.is_some() || manifest.attributes.is_some() {
        identity["auxiliaryProjection"] = "semantic-ndc-v1".into();
        for (name, kind, source) in [
            (
                "semanticTypes",
                auxiliary::Kind::SemanticTypes,
                manifest.semantic_types.as_ref(),
            ),
            (
                "attributes",
                auxiliary::Kind::Attributes,
                manifest.attributes.as_ref(),
            ),
        ] {
            if let Some(source) = source {
                identity[name] = kind.identity(source);
            }
        }
    }
    if let Some(source) = manifest.relationships.as_ref() {
        identity["relationshipProjection"] = "relation-v1".into();
        identity["relationships"] = relationships::identity(source);
    }
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
        source.rows == manifest.expected_rows,
        "RxNorm raw row count does not match expectedRows"
    );
    sqlx::query("CREATE INDEX rxnorm_input_concept ON rxnorm_input(rxcui)")
        .execute(&mut *tx)
        .await?;
    // Canonical normalized names are not ISPREF, PSN or arbitrary source names.
    let foreign_primary: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_input WHERE sab='RXNORM' AND suppress='N' AND tty NOT IN ('SY','PSN','TMSY') AND lat<>'ENG')").fetch_one(&mut *tx).await?;
    ensure!(
        !foreign_primary,
        "Active RxNorm primary names require LAT=ENG"
    );
    sqlx::query("CREATE TEMP TABLE rxnorm_primary ON COMMIT DROP AS SELECT DISTINCT rxcui,tty,term FROM rxnorm_input WHERE sab='RXNORM' AND suppress='N' AND tty NOT IN ('SY','PSN','TMSY')").execute(&mut *tx).await?;
    let ambiguous: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM rxnorm_primary GROUP BY rxcui HAVING count(*)<>1)",
    )
    .fetch_one(&mut *tx)
    .await?;
    ensure!(!ambiguous, "Ambiguous active RxNorm primary type/name");
    sqlx::query("CREATE UNIQUE INDEX rxnorm_primary_concept ON rxnorm_primary(rxcui)")
        .execute(&mut *tx)
        .await?;
    let missing: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_input a WHERE a.sab='RXNORM' AND a.suppress='N' AND NOT EXISTS(SELECT 1 FROM rxnorm_primary p WHERE p.rxcui=a.rxcui))").fetch_one(&mut *tx).await?;
    ensure!(
        !missing,
        "Active RxNorm concept is missing a canonical primary name"
    );
    let concepts: i64 = sqlx::query_scalar("SELECT count(*) FROM rxnorm_primary")
        .fetch_one(&mut *tx)
        .await?;
    ensure!(
        concepts as u64 == manifest.expected_concepts,
        "RxNorm active concept count does not match expectedConcepts"
    );
    // Bound each projected JSON document before aggregation; never truncate aliases/tags.
    let oversized: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) WHERE a.sab='RXNORM' AND a.suppress='N' AND a.tty IN ('PSN','SY','TMSY') GROUP BY a.rxcui HAVING sum(256::bigint+8::bigint*(octet_length(a.term)+octet_length(a.tty)+octet_length(a.lat)))>4194304)").fetch_one(&mut *tx).await?;
    ensure!(
        !oversized,
        "RxNorm concept designation projection exceeds 4 MiB budget; no aliases truncated"
    );
    let oversized_tags: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM (SELECT DISTINCT a.rxcui,'SAB' AS property,a.sab AS value FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) UNION ALL SELECT DISTINCT a.rxcui,'TTY',a.tty FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) WHERE a.sab='RXNORM' AND a.tty<>'SY') t GROUP BY rxcui HAVING sum(256::bigint+8::bigint*octet_length(value))>4194304)").fetch_one(&mut *tx).await?;
    ensure!(
        !oversized_tags,
        "RxNorm concept property projection exceeds 4 MiB budget"
    );
    let auxiliary = auxiliary::stage(
        &mut tx,
        path.parent().unwrap_or_else(|| Path::new(".")),
        manifest.semantic_types.as_ref(),
        manifest.attributes.as_ref(),
    )
    .await?;
    let relationships = relationships::stage(
        &mut tx,
        path.parent().unwrap_or_else(|| Path::new(".")),
        manifest.relationships.as_ref(),
    )
    .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('term-squid/rxnorm:'||current_schema()||':'||$1,0))").bind(&manifest.version).execute(&mut *tx).await?;
    let existing: Option<(Uuid,Option<String>,Option<String>)> = sqlx::query_as("SELECT c.id,c.native_provider,r.fingerprint FROM code_systems c LEFT JOIN rxnorm_releases r ON r.code_system_id=c.id WHERE c.url=$1 AND c.version=$2").bind(SYSTEM).bind(&manifest.version).fetch_optional(&mut *tx).await?;
    if let Some((id, provider, stored)) = existing {
        ensure!(
            provider.as_deref() == Some("rxnorm-rrf")
                && stored.as_deref() == Some(fingerprint.as_str()),
            "Conflicting immutable RxNorm release already exists; nothing replaced"
        );
        tx.rollback().await?;
        return Ok(ImportResult {
            code_system_id: Some(id),
            version: manifest.version,
            fingerprint,
            dry_run,
            unchanged: true,
            source,
            concepts: concepts as u64,
            auxiliary_sources: auxiliary.result_sources(),
            relationship_source: relationships.result_source(),
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
            concepts: concepts as u64,
            auxiliary_sources: auxiliary.result_sources(),
            relationship_source: relationships.result_source(),
        });
    }
    let id = Uuid::new_v4();
    let mut metadata = metadata(id, &manifest.version, release);
    auxiliary.metadata(&mut metadata);
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider,fhir_version,updated_at) VALUES($1,$2,$3,'active','RxNorm',$4,'rxnorm-rrf','R4',clock_timestamp())").bind(id).bind(SYSTEM).bind(&manifest.version).bind(sqlx::types::Json(metadata)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO rxnorm_atoms(code_system_id,rxaui,rxcui,lat,sab,tty,term,suppress,fields) SELECT $1,rxaui,rxcui,lat,sab,tty,term,suppress,fields FROM rxnorm_input").bind(id).execute(&mut *tx).await?;
    sqlx::query("WITH tags AS MATERIALIZED (SELECT rxcui,jsonb_agg(jsonb_build_object('code',property,'valueCode',value) ORDER BY property,value) AS properties FROM (SELECT DISTINCT a.rxcui,'SAB' AS property,a.sab AS value FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) UNION ALL SELECT DISTINCT a.rxcui,'TTY',a.tty FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) WHERE a.sab='RXNORM' AND a.tty<>'SY') t GROUP BY rxcui), aliases AS MATERIALIZED (SELECT rxcui,jsonb_agg(jsonb_build_object('language','en','use',jsonb_build_object('system',$2::text,'code',tty),'value',term) ORDER BY tty,term) AS designations FROM (SELECT DISTINCT a.rxcui,a.tty,a.term FROM rxnorm_input a JOIN rxnorm_primary p USING(rxcui) WHERE a.sab='RXNORM' AND a.suppress='N' AND a.lat='ENG' AND a.tty IN ('PSN','SY','TMSY')) a GROUP BY rxcui) INSERT INTO concepts(code_system_id,code,display,properties,designations) SELECT $1,p.rxcui,p.term,jsonb_build_array(jsonb_build_object('code','inactive','valueBoolean',false),jsonb_build_object('code','source-table','valueString','RXNCONSO.RRF'),jsonb_build_object('code','rxnorm-status','valueCode','active'),jsonb_build_object('code','preferredTTY','valueCode',p.tty))||COALESCE(t.properties,'[]'::jsonb),COALESCE(a.designations,'[]'::jsonb) FROM rxnorm_primary p LEFT JOIN tags t USING(rxcui) LEFT JOIN aliases a USING(rxcui)").bind(id).bind(SYSTEM).execute(&mut *tx).await?;
    auxiliary.publish(&mut tx, id).await?;
    relationships.publish(&mut tx, id).await?;
    let mut provenance = json!({"identity":identity,"source":source,"fullMonthlyAttestation":true,"scope":"Current-active SAB=RXNORM normalized concepts only; no historical completeness, archived/remapped validation, STY, relationships, NDC or UNII attributes","rawFields":"All 18 source columns and all source atoms retained verbatim; RXCUI is the public identity, never RXAUI or CODE"});
    if auxiliary.enabled() {
        provenance["auxiliarySources"] = auxiliary.sources.clone();
        provenance["scope"]="Current-active SAB=RXNORM normalized concepts with selected attested semantic/NDC sources; no historical completeness, archived/remapped validation, relationships, inferred NDC mappings or UNII attributes".into();
    }
    if relationships.loaded {
        provenance["scope"]="Current-active SAB=RXNORM normalized names with attested direct RXNREL facts; semantic/NDC knowledge only where its separate source availability is declared. Historical completeness, archived/remapped validation, transitive hierarchy, subsumption and inferred mappings remain unavailable".into();
        provenance["relationshipSource"] = relationships.source.clone();
        provenance["relationshipScope"]="Direct source RXNREL facts only, with original direction, namespaces and suppression markers; no transitive hierarchy, subsumption, lookup adjacency or generated ConceptMap".into();
        sqlx::query("INSERT INTO rxnorm_releases(code_system_id,version,release_date,fingerprint,provenance,semantic_types_loaded,attributes_loaded,auxiliary_sources,relationships_loaded,relationship_source) VALUES($1,$2,$3,$4,$5,$6,$7,$8,true,$9)").bind(id).bind(&manifest.version).bind(release).bind(&fingerprint).bind(sqlx::types::Json(provenance)).bind(auxiliary.semantic_types).bind(auxiliary.attributes).bind(sqlx::types::Json(&auxiliary.sources)).bind(sqlx::types::Json(&relationships.source)).execute(&mut *tx).await?;
    } else if auxiliary.enabled() {
        sqlx::query("INSERT INTO rxnorm_releases(code_system_id,version,release_date,fingerprint,provenance,semantic_types_loaded,attributes_loaded,auxiliary_sources) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(id).bind(&manifest.version).bind(release).bind(&fingerprint).bind(sqlx::types::Json(provenance)).bind(auxiliary.semantic_types).bind(auxiliary.attributes).bind(sqlx::types::Json(&auxiliary.sources)).execute(&mut *tx).await?;
    } else {
        sqlx::query("INSERT INTO rxnorm_releases(code_system_id,version,release_date,fingerprint,provenance) VALUES($1,$2,$3,$4,$5)").bind(id).bind(&manifest.version).bind(release).bind(&fingerprint).bind(sqlx::types::Json(provenance)).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(ImportResult {
        code_system_id: Some(id),
        version: manifest.version,
        fingerprint,
        dry_run,
        unchanged: false,
        source,
        concepts: concepts as u64,
        auxiliary_sources: auxiliary.result_sources(),
        relationship_source: relationships.result_source(),
    })
}

fn metadata(id: Uuid, version: &str, release: NaiveDate) -> Value {
    json!({"resourceType":"CodeSystem","id":id,"url":SYSTEM,"version":version,"date":release.to_string(),
    "status":"active","name":"RxNorm","publisher":"US National Library of Medicine",
    "content":"not-present","caseSensitive":true,
    "description":"Native current-active SAB=RXNORM normalized-name projection from a full monthly RXNCONSO.RRF; historical, archived and remapped knowledge is unavailable.",
    "property":[
        {"code":"inactive","uri":"http://hl7.org/fhir/concept-properties#inactive","type":"boolean"},
        {"code":"source-table","uri":"urn:term-squid:rxnorm:source-table","type":"string"},
        {"code":"rxnorm-status","uri":"urn:term-squid:rxnorm:status","type":"code"},
        {"code":"preferredTTY","uri":"urn:term-squid:rxnorm:preferredTTY","type":"code"},
        {"code":"TTY","uri":"urn:term-squid:rxnorm:TTY","type":"code","description":"All RXNORM atom term types except SY, including suppressed atom types"},
        {"code":"SAB","uri":"urn:term-squid:rxnorm:SAB","type":"code","description":"All source abbreviations attached to this RXCUI in the full monthly atom table"}
    ]})
}
