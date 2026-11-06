//! Fresh, bounded operational readiness; not clinical content or conformance attestation.
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const PROBES: usize = 4;
const DEADLINE: Duration = Duration::from_secs(2);
const NATIVE_LIMIT: i64 = 10_000;
static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

const TABLES: &[&str] = &[
    "_sqlx_migrations",
    "code_systems",
    "value_sets",
    "concept_maps",
    "concepts",
    "closure_table",
    "terminology_generation",
    "snomed_releases",
    "snomed_descriptions",
    "snomed_relationships",
    "snomed_language_members",
    "snomed_simple_refsets",
    "snomed_simple_members",
    "loinc_releases",
    "loinc_terms",
    "loinc_primary_links",
    "rxnorm_releases",
    "rxnorm_atoms",
    "rxnorm_semantic_types",
    "rxnorm_attributes",
    "rxnorm_relationships",
];
// Only the repository's generation-tracking triggers belong to this check.
const TRIGGERS: &[(&str, &str)] = &[
    ("code_systems", "code_system_generation"),
    ("value_sets", "value_set_generation"),
    ("concept_maps", "concept_map_generation"),
    ("concepts", "concept_projection_generation"),
    ("closure_table", "closure_generation"),
    ("snomed_releases", "snomed_release_generation"),
    ("snomed_descriptions", "snomed_description_generation"),
    ("snomed_relationships", "snomed_relationship_generation"),
    ("snomed_language_members", "snomed_language_generation"),
    ("snomed_simple_refsets", "snomed_simple_refset_generation"),
    ("snomed_simple_members", "snomed_simple_member_generation"),
    ("loinc_releases", "loinc_release_generation"),
    ("loinc_terms", "loinc_term_generation"),
    ("loinc_primary_links", "loinc_primary_link_generation"),
    ("rxnorm_releases", "rxnorm_release_generation"),
    ("rxnorm_atoms", "rxnorm_atom_generation"),
    ("rxnorm_semantic_types", "rxnorm_semantic_type_generation"),
    ("rxnorm_attributes", "rxnorm_attribute_generation"),
    ("rxnorm_relationships", "rxnorm_relationship_generation"),
];

#[derive(Clone)]
struct Readiness {
    pool: PgPool,
    admission: Arc<Semaphore>,
}

pub fn router(pool: PgPool) -> Router {
    routes(Readiness {
        pool,
        admission: Arc::new(Semaphore::new(PROBES)),
    })
}

fn routes(readiness: Readiness) -> Router {
    Router::new()
        .route("/ready", get(check))
        .with_state(readiness)
}

async fn check(State(readiness): State<Readiness>) -> Response {
    let Ok(_permit) = readiness.admission.try_acquire() else {
        return answer(false);
    };
    // Includes checkout and every statement/commit. No cached readiness or stale fallback.
    let ready = match tokio::time::timeout(DEADLINE, probe(&readiness.pool)).await {
        Ok(Ok(ready)) => ready,
        Ok(Err(error)) => {
            tracing::debug!(%error, "Readiness database probe failed");
            false
        }
        Err(_) => false,
    };
    answer(ready)
}

fn answer(ready: bool) -> Response {
    (
        if ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        },
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "status":if ready {"ready"} else {"not-ready"},
            "service":"term-squid",
            "version":env!("CARGO_PKG_VERSION"),
        })),
    )
        .into_response()
}

async fn probe(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::raw_sql("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY; SET LOCAL statement_timeout = '1s'")
        .execute(&mut *tx).await?;
    let healthy = inspect(&mut tx).await?;
    tx.commit().await?;
    Ok(healthy)
}

async fn inspect(tx: &mut Transaction<'_, Postgres>) -> Result<bool, sqlx::Error> {
    // Do not accidentally accept a table from a later search_path schema when the
    // application's own migrated schema is incomplete. Admin/session tables are unrelated.
    let tables: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM unnest($1::text[]) expected(name) JOIN pg_class c ON c.oid=to_regclass(expected.name) JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind='r'")
        .bind(TABLES).fetch_one(&mut **tx).await?;
    if tables != TABLES.len() as i64 {
        return Ok(false);
    }
    let expected: Vec<_> = MIGRATIONS
        .iter()
        .filter(|migration| migration.migration_type.is_up_migration())
        .collect();
    // The extra row detects unknown newer/foreign migrations without decoding an
    // unbounded ledger. A malformed checksum is masked before transfer to Rust.
    let applied: Vec<(i64, bool, Option<Vec<u8>>)> = sqlx::query_as("SELECT version,success,CASE WHEN octet_length(checksum)=48 THEN checksum END FROM _sqlx_migrations ORDER BY version LIMIT $1")
        .bind(expected.len() as i64+1).fetch_all(&mut **tx).await?;
    if applied.len() != expected.len()
        || applied
            .iter()
            .zip(&expected)
            .any(|((version, success, checksum), migration)| {
                !success
                    || *version != migration.version
                    || checksum.as_deref() != Some(migration.checksum.as_ref())
            })
    {
        return Ok(false);
    }
    let generation: Option<(i64, bool, bool)> = sqlx::query_as(
        "SELECT generation,maps_tracked,hierarchies_tracked FROM terminology_generation WHERE singleton",
    )
    .fetch_optional(&mut **tx)
    .await?;
    if !matches!(generation, Some((generation, true, true)) if generation > 0) {
        return Ok(false);
    }
    let (relations, names): (Vec<_>, Vec<_>) = TRIGGERS.iter().copied().unzip();
    let triggers:i64=sqlx::query_scalar("SELECT count(*)::bigint FROM unnest($1::text[],$2::text[]) expected(relation,name) JOIN pg_trigger t ON t.tgrelid=to_regclass(expected.relation) AND t.tgname=expected.name WHERE NOT t.tgisinternal AND t.tgenabled IN ('O','A') AND t.tgfoid=to_regprocedure('advance_terminology_generation()') AND t.tgtype=60")
        .bind(relations).bind(names).fetch_one(&mut **tx).await?;
    if triggers != TRIGGERS.len() as i64 {
        return Ok(false);
    }
    // Only small release metadata is inspected, never raw clinical files or full
    // concept projections. Structural readiness does not certify clinical completeness.
    let (native_count,native_consistent):(i64,bool)=sqlx::query_as("WITH native AS MATERIALIZED (SELECT id,url,version,native_provider FROM code_systems WHERE native_provider IS NOT NULL ORDER BY id LIMIT $1) SELECT count(*)::bigint,COALESCE(bool_and(COALESCE(CASE n.native_provider WHEN 'snomed-rf2' THEN n.url='http://snomed.info/sct' AND s.code_system_id IS NOT NULL AND n.version=('http://snomed.info/sct/'||s.edition_module||'/version/'||to_char(s.release_date,'YYYYMMDD')) AND s.simple_refsets_loaded=(s.simple_refset_source<>'{}'::jsonb) AND (NOT s.simple_refsets_loaded OR snomed_simple_refset_source_valid(s.simple_refset_source,to_char(s.release_date,'YYYYMMDD'))) WHEN 'loinc-csv' THEN n.url='http://loinc.org' AND l.code_system_id IS NOT NULL AND n.version=l.version AND l.primary_links_loaded=(l.primary_link_source<>'{}'::jsonb) AND (NOT l.primary_links_loaded OR loinc_primary_link_source_valid(l.primary_link_source,l.version)) WHEN 'rxnorm-rrf' THEN n.url='http://www.nlm.nih.gov/research/umls/rxnorm' AND r.code_system_id IS NOT NULL AND n.version=r.version AND n.version=to_char(r.release_date,'MMDDYYYY') AND r.relationships_loaded=(r.relationship_source<>'{}'::jsonb) AND (NOT r.relationships_loaded OR rxnorm_relationship_source_valid(r.relationship_source,r.version)) AND jsonb_typeof(r.auxiliary_sources)='object' AND octet_length(r.auxiliary_sources::text)<=8192 AND r.auxiliary_sources-'RXNSTY.RRF'-'RXNSAT.RRF'='{}'::jsonb AND r.semantic_types_loaded=(r.auxiliary_sources?'RXNSTY.RRF') AND r.attributes_loaded=(r.auxiliary_sources?'RXNSAT.RRF') AND (NOT r.semantic_types_loaded OR rxnorm_auxiliary_source_valid(r.auxiliary_sources->'RXNSTY.RRF','RXNSTY.RRF',r.version,1)) AND (NOT r.attributes_loaded OR rxnorm_auxiliary_source_valid(r.auxiliary_sources->'RXNSAT.RRF','RXNSAT.RRF',r.version,0)) ELSE false END,false)),true) FROM native n LEFT JOIN snomed_releases s ON s.code_system_id=n.id LEFT JOIN loinc_releases l ON l.code_system_id=n.id LEFT JOIN rxnorm_releases r ON r.code_system_id=n.id")
        .bind(NATIVE_LIMIT+1).fetch_one(&mut **tx).await?;
    if native_count > NATIVE_LIMIT || !native_consistent {
        return Ok(false);
    }
    // This registry is bounded metadata (at most 64 declarations per release),
    // not a scan of the member corpus. Missing/extra declarations fail closed.
    let registries: bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM snomed_releases r JOIN code_systems n ON n.id=r.code_system_id WHERE n.native_provider='snomed-rf2' AND ((SELECT count(*) FROM snomed_simple_refsets f WHERE f.code_system_id=r.code_system_id) <> CASE WHEN r.simple_refsets_loaded THEN jsonb_array_length(r.simple_refset_source->'refsetIds') ELSE 0 END OR EXISTS(SELECT 1 FROM snomed_simple_refsets f WHERE f.code_system_id=r.code_system_id AND NOT COALESCE(r.simple_refset_source->'refsetIds' ? f.refset_id,false))))").fetch_one(&mut **tx).await?;
    Ok(registries)
}

#[cfg(test)]
#[path = "readiness_tests.rs"]
mod tests;
