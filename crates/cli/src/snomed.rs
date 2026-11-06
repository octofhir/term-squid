//! Atomic native RF2 Snapshot loading, separate from finite FHIR JSON imports.
mod reader;
mod simple_refsets;
#[cfg(test)]
mod tests;

use anyhow::{ensure, Context, Result};
use reader::Kind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{
    collections::BTreeMap,
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
#[serde(deny_unknown_fields)]
pub struct Sources {
    pub concepts: Source,
    pub descriptions: Source,
    pub relationships: Source,
    pub language: Source,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub complete_snapshot: bool,
    pub edition_module: String,
    pub release: String,
    pub dialect: String,
    pub language: String,
    pub sources: Sources,
    #[serde(default)]
    simple_refsets: Option<simple_refsets::Source>,
}
#[derive(Clone, Debug, Serialize)]
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
    pub sources: BTreeMap<&'static str, SourceStats>,
}

/// Sources are local, already extracted complete merged Snapshots. No network access.
pub async fn import_manifest(pool: &PgPool, path: &Path, dry_run: bool) -> Result<ImportResult> {
    ensure!(
        std::fs::metadata(path)?.is_file(),
        "RF2 manifest must be a regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(65537)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "RF2 manifest exceeds 64 KiB");
    let manifest: Manifest = serde_json::from_slice(&bytes).context("Invalid RF2 manifest")?;
    ensure!(
        manifest.format == "snomed-rf2-snapshot-v1" && manifest.complete_snapshot,
        "An explicitly attested complete RF2 Snapshot is required"
    );
    reader::sctid(&manifest.edition_module)?;
    reader::sctid(&manifest.dialect)?;
    reader::language(&manifest.language)?;
    let release = reader::date(&manifest.release)?;
    if let Some(source) = &manifest.simple_refsets {
        source.validate(&manifest.release)?;
    }
    let version = format!(
        "http://snomed.info/sct/{}/version/{}",
        manifest.edition_module, manifest.release
    );
    let inputs = [
        (Kind::Concepts, &manifest.sources.concepts),
        (Kind::Descriptions, &manifest.sources.descriptions),
        (Kind::Relationships, &manifest.sources.relationships),
        (Kind::Language, &manifest.sources.language),
    ];
    let mut identities = BTreeMap::new();
    for (kind, source) in inputs {
        ensure!(
            source.sha256.len() == 64
                && source
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Expected lowercase source SHA-256"
        );
        let name = source
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid RF2 source filename")?;
        ensure!(
            name.contains("Snapshot") && !name.contains("Delta") && !name.contains("Full"),
            "RF2 sources must be named Snapshot files, not Full or Delta"
        );
        ensure!(
            name.ends_with(&format!("_{}.txt", manifest.release)),
            "RF2 source filename must match the declared release date"
        );
        identities.insert(kind.name(), source.sha256.as_str());
    }
    let mut identity = serde_json::json!({"format":manifest.format,"editionModule":manifest.edition_module,"release":manifest.release,"dialect":manifest.dialect,"language":manifest.language,"sources":identities});
    if let Some(source) = &manifest.simple_refsets {
        identity["simpleRefsetProjection"] = "concept-simple-v1".into();
        identity["simpleRefsets"] = source.identity();
    }
    let fingerprint = hex::encode(Sha256::digest(serde_json::to_vec(&identity)?));
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout='15min'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL lock_timeout='5s'")
        .execute(&mut *tx)
        .await?;
    let mut sources = BTreeMap::new();
    for (kind, source) in inputs {
        let file = if source.path.is_absolute() {
            source.path.clone()
        } else {
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&source.path)
        };
        let stats = reader::copy_source(&mut tx, kind, source.clone(), file, release).await?;
        sources.insert(kind.name(), stats);
    }
    validate_staging(&mut tx, &manifest).await?;
    validate_hierarchy(&mut tx).await?;
    let simple_refsets = if let Some(source) = &manifest.simple_refsets {
        let prepared = source.stage(&mut tx, path, release).await?;
        sources.insert("simpleRefsets", prepared.stats.clone());
        Some(prepared)
    } else {
        None
    };
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('term-squid/rf2:'||current_schema()||':'||$1,0))")
        .bind(&version)
        .execute(&mut *tx)
        .await?;
    let existing:Option<(Uuid,Option<String>,Option<String>)>=sqlx::query_as("SELECT c.id,c.native_provider,r.fingerprint FROM code_systems c LEFT JOIN snomed_releases r ON r.code_system_id=c.id WHERE c.url='http://snomed.info/sct' AND c.version=$1").bind(&version).fetch_optional(&mut *tx).await?;
    if let Some((id, provider, stored)) = existing {
        ensure!(
            provider.as_deref() == Some("snomed-rf2")
                && stored.as_deref() == Some(fingerprint.as_str()),
            "Conflicting immutable SNOMED release already exists; nothing replaced"
        );
        tx.rollback().await?;
        return Ok(ImportResult {
            code_system_id: Some(id),
            version,
            fingerprint,
            dry_run,
            unchanged: true,
            sources,
        });
    }
    if dry_run {
        tx.rollback().await?;
        return Ok(ImportResult {
            code_system_id: None,
            version,
            fingerprint,
            dry_run,
            unchanged: false,
            sources,
        });
    }
    let id = Uuid::new_v4();
    let metadata = serde_json::json!({"resourceType":"CodeSystem","id":id,"url":"http://snomed.info/sct","version":version,"status":"active","name":"SNOMEDCT","date":release.to_string(),"content":"not-present","caseSensitive":true,"hierarchyMeaning":"is-a"});
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider,fhir_version,updated_at) VALUES($1,'http://snomed.info/sct',$2,'active','SNOMEDCT',$3,'snomed-rf2','R4',clock_timestamp())")
        .bind(id).bind(&version).bind(sqlx::types::Json(metadata)).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,c.id,COALESCE(p.term,f.term),jsonb_build_array(jsonb_build_object('code','inactive','valueBoolean',c.active='0'),jsonb_build_object('code','moduleId','valueCode',c.module_id),jsonb_build_object('code','definitionStatusId','valueCode',c.definition_status_id),jsonb_build_object('code','effectiveTime','valueString',c.effective_time)) FROM rf2_concepts c JOIN rf2_fsn f ON f.concept_id=c.id LEFT JOIN rf2_preferred p ON p.concept_id=c.id")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO snomed_descriptions(code_system_id,id,effective_time,active,module_id,concept_code,language_code,type_id,term,case_significance_id) SELECT $1,id,to_date(effective_time,'YYYYMMDD'),active='1',module_id,concept_id,language_code,type_id,term,case_significance_id FROM rf2_descriptions")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO snomed_relationships(code_system_id,id,effective_time,active,module_id,source_code,destination_code,relationship_group,type_id,characteristic_type_id,modifier_id) SELECT $1,id,to_date(effective_time,'YYYYMMDD'),active='1',module_id,source_id,destination_id,relationship_group::integer,type_id,characteristic_type_id,modifier_id FROM rf2_relationships")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO snomed_language_members(code_system_id,id,effective_time,active,module_id,refset_id,description_id,acceptability_id) SELECT $1,id::uuid,to_date(effective_time,'YYYYMMDD'),active='1',module_id,refset_id,description_id,acceptability_id FROM rf2_language")
        .bind(id).execute(&mut *tx).await?;
    let mut provenance = serde_json::json!({"identity":identity,"sources":sources,"completeSnapshotAttestation":true,"scope":"Concept, Description, inferred Relationship and Language Snapshot; no Full, Delta, OWL, concrete relationships or other refsets"});
    if simple_refsets.is_some() {
        provenance["scope"] = "Concept, Description, inferred Relationship, Language and declared concept-only Simple Snapshots; no Full, Delta, OWL, concrete relationships or other refsets".into();
    }
    sqlx::query("INSERT INTO snomed_releases(code_system_id,edition_module,release_date,dialect,language,fingerprint,provenance) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(&manifest.edition_module).bind(release).bind(&manifest.dialect).bind(&manifest.language).bind(&fingerprint).bind(sqlx::types::Json(provenance)).execute(&mut *tx).await?;
    if let Some(prepared) = simple_refsets {
        prepared.publish(&mut tx, id).await?;
    }
    tx.commit().await?;
    Ok(ImportResult {
        code_system_id: Some(id),
        version,
        fingerprint,
        dry_run,
        unchanged: false,
        sources,
    })
}

async fn require_empty(
    tx: &mut Transaction<'_, Postgres>,
    query: &'static str,
    message: &str,
) -> Result<()> {
    let found: bool = sqlx::query_scalar(query).fetch_one(&mut **tx).await?;
    ensure!(!found, "{message}");
    Ok(())
}
async fn validate_staging(tx: &mut Transaction<'_, Postgres>, manifest: &Manifest) -> Result<()> {
    sqlx::query("CREATE INDEX rf2_description_concept ON rf2_descriptions(concept_id)")
        .execute(&mut **tx)
        .await?;
    sqlx::query("CREATE INDEX rf2_language_description ON rf2_language(description_id)")
        .execute(&mut **tx)
        .await?;
    sqlx::query("CREATE INDEX rf2_parent_destination ON rf2_relationships(destination_id) WHERE active='1' AND type_id='116680003'").execute(&mut **tx).await?;
    for table in [
        "rf2_concepts",
        "rf2_descriptions",
        "rf2_relationships",
        "rf2_language",
    ] {
        // Identifiers come exclusively from the fixed staging-table list above.
        sqlx::query(sqlx::AssertSqlSafe(format!("ANALYZE {table}")))
            .execute(&mut **tx)
            .await?;
    }
    require_empty(tx,"SELECT EXISTS(SELECT 1 FROM rf2_descriptions d LEFT JOIN rf2_concepts c ON c.id=d.concept_id WHERE c.id IS NULL)","Description references a missing concept; supply a complete merged edition").await?;
    require_empty(tx,"SELECT EXISTS(SELECT 1 FROM rf2_relationships r LEFT JOIN rf2_concepts a ON a.id=r.source_id LEFT JOIN rf2_concepts b ON b.id=r.destination_id WHERE a.id IS NULL OR b.id IS NULL OR (r.active='1' AND (a.active<>'1' OR b.active<>'1')))","Relationship has missing/inactive endpoints; supply a consistent complete edition").await?;
    require_empty(tx,"SELECT EXISTS(SELECT 1 FROM rf2_language l LEFT JOIN rf2_descriptions d ON d.id=l.description_id WHERE d.id IS NULL)","Language member references a missing description").await?;
    let module: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rf2_concepts WHERE module_id=$1)")
            .bind(&manifest.edition_module)
            .fetch_one(&mut **tx)
            .await?;
    ensure!(module, "Edition module is absent from the Concept Snapshot");
    let dialect: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM rf2_language WHERE active='1' AND refset_id=$1)",
    )
    .bind(&manifest.dialect)
    .fetch_one(&mut **tx)
    .await?;
    ensure!(dialect, "Selected dialect has no active language members");
    sqlx::query("CREATE TEMP TABLE rf2_preferred ON COMMIT DROP AS SELECT DISTINCT d.concept_id,d.id,d.term FROM rf2_descriptions d JOIN rf2_language l ON l.description_id=d.id WHERE d.active='1' AND d.language_code=$1 AND d.type_id='900000000000013009' AND l.active='1' AND l.refset_id=$2 AND l.acceptability_id='900000000000548007'")
        .bind(&manifest.language).bind(&manifest.dialect).execute(&mut **tx).await?;
    require_empty(
        tx,
        "SELECT EXISTS(SELECT 1 FROM rf2_preferred GROUP BY concept_id HAVING count(*)>1)",
        "Ambiguous preferred synonyms in the selected dialect",
    )
    .await?;
    sqlx::query("CREATE UNIQUE INDEX rf2_preferred_concept ON rf2_preferred(concept_id)")
        .execute(&mut **tx)
        .await?;
    let ambiguous:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rf2_descriptions WHERE active='1' AND type_id='900000000000003001' AND language_code=$1 GROUP BY concept_id HAVING count(*)>1)").bind(&manifest.language).fetch_one(&mut **tx).await?;
    ensure!(!ambiguous, "Multiple active FSNs for the selected language");
    sqlx::query("CREATE TEMP TABLE rf2_fsn ON COMMIT DROP AS SELECT DISTINCT ON(concept_id) concept_id,id,term,active FROM rf2_descriptions WHERE type_id='900000000000003001' AND language_code=$1 ORDER BY concept_id,active DESC,effective_time DESC,id")
        .bind(&manifest.language).execute(&mut **tx).await?;
    sqlx::query("CREATE UNIQUE INDEX rf2_fsn_concept ON rf2_fsn(concept_id)")
        .execute(&mut **tx)
        .await?;
    require_empty(tx,"SELECT EXISTS(SELECT 1 FROM rf2_concepts c LEFT JOIN rf2_fsn f ON f.concept_id=c.id WHERE f.id IS NULL OR (c.active='1' AND f.active<>'1'))","Concept has no FSN in the selected language, or an active concept has no active FSN").await?;
    Ok(())
}

async fn validate_hierarchy(tx: &mut Transaction<'_, Postgres>) -> Result<()> {
    // Kahn elimination in temporary tables: linear-size graph state, not transitive closure.
    sqlx::query("CREATE TEMP TABLE rf2_degree ON COMMIT DROP AS SELECT c.id AS code,count(r.id)::bigint AS parents FROM rf2_concepts c LEFT JOIN rf2_relationships r ON r.source_id=c.id AND r.active='1' AND r.type_id='116680003' GROUP BY c.id").execute(&mut **tx).await?;
    sqlx::query("CREATE UNIQUE INDEX rf2_degree_code ON rf2_degree(code)")
        .execute(&mut **tx)
        .await?;
    sqlx::query("CREATE INDEX rf2_degree_roots ON rf2_degree(code) WHERE parents=0")
        .execute(&mut **tx)
        .await?;
    sqlx::query("CREATE TEMP TABLE rf2_frontier(code text PRIMARY KEY) ON COMMIT DROP")
        .execute(&mut **tx)
        .await?;
    let mut remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM rf2_degree")
        .fetch_one(&mut **tx)
        .await?;
    for _ in 0..=64 {
        sqlx::query("TRUNCATE rf2_frontier")
            .execute(&mut **tx)
            .await?;
        let removed =
            sqlx::query("INSERT INTO rf2_frontier SELECT code FROM rf2_degree WHERE parents=0")
                .execute(&mut **tx)
                .await?
                .rows_affected() as i64;
        ensure!(removed > 0, "Inferred is-a hierarchy contains a cycle");
        sqlx::query("DELETE FROM rf2_degree d USING rf2_frontier f WHERE d.code=f.code")
            .execute(&mut **tx)
            .await?;
        remaining -= removed;
        if remaining == 0 {
            return Ok(());
        }
        sqlx::query("UPDATE rf2_degree d SET parents=d.parents-p.n FROM(SELECT r.source_id,count(*)::bigint AS n FROM rf2_relationships r JOIN rf2_frontier f ON f.code=r.destination_id WHERE r.active='1' AND r.type_id='116680003' GROUP BY r.source_id)p WHERE d.code=p.source_id").execute(&mut **tx).await?;
    }
    anyhow::bail!("Inferred hierarchy exceeds 64 levels")
}
