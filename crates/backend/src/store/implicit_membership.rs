//! A single data statement for implicit whole-system / descendant membership.
//! Keep metadata, existence checks and the complete bounded walk in one snapshot.
use super::{
    native_valueset::{selected_release_sql, unsupported, MEMBERSHIP_WALK, PAGE_BYTES},
    ValueSetPage, ValueSetResult,
};
use crate::error::AppError;
use sqlx::{Postgres, QueryBuilder, Transaction};
use term_squid_core::{
    snomed::{self, ImplicitValueSet},
    Canonical, Member,
};

#[derive(sqlx::FromRow)]
struct Row {
    native_provider: Option<String>,
    anchor_known: bool,
    walk_exceeded: bool,
    page_exceeded: bool,
    version: Option<String>,
    code: Option<String>,
    display: Option<String>,
    inactive: bool,
}

pub(super) async fn find(
    tx: &mut Transaction<'_, Postgres>,
    root: &Canonical,
    definition: &ImplicitValueSet,
    context: Option<(&str, &str)>,
    system: &str,
    system_version: Option<&str>,
    code: &str,
) -> Result<ValueSetResult, AppError> {
    if definition.constraint.is_some() {
        return Err(unsupported(
            "ECL requires the guarded general native provider",
        ));
    }
    let release = definition.system_version.as_deref().or_else(|| {
        context
            .filter(|(url, _)| *url == snomed::SYSTEM)
            .map(|(_, version)| version)
    });
    let mut sql = QueryBuilder::new("WITH RECURSIVE selected AS MATERIALIZED (");
    selected_release_sql(&mut sql, release, true);
    sql.push("), input AS (SELECT ").push_bind(code.to_owned()).push("::text AS code,")
        .push_bind(definition.ancestor.clone()).push("::text AS ancestor,")
        .push_bind(system.to_owned()).push("::text AS system,")
        .push_bind(system_version.map(str::to_owned)).push("::text AS version), ")
        .push("context AS MATERIALIZED (SELECT s.*, (i.ancestor IS NULL OR EXISTS(SELECT 1 FROM concepts c WHERE c.code_system_id=s.id AND c.code=i.ancestor)) AS anchor_known FROM selected s CROSS JOIN input i), ")
        .push("walk(code) AS (SELECT i.code FROM input i CROSS JOIN context s WHERE i.ancestor IS NOT NULL AND s.anchor_known AND s.native_provider='snomed-rf2' UNION SELECT r.destination_code::text FROM walk w CROSS JOIN context s CROSS JOIN LATERAL (SELECT destination_code FROM snomed_relationships r WHERE r.code_system_id=s.id AND r.source_code=w.code AND r.active AND r.type_id='116680003' AND r.characteristic_type_id='900000000000011006' OFFSET 0) r), ")
        .push(format!("bounded AS MATERIALIZED (SELECT code FROM walk LIMIT {}), ",MEMBERSHIP_WALK+1))
        .push("matched AS (SELECT s.version,c.code,c.display,c.properties FROM context s CROSS JOIN input i JOIN concepts c ON c.code_system_id=s.id AND c.code=i.code WHERE s.native_provider='snomed-rf2' AND s.anchor_known AND i.system=")
        .push_bind(snomed::SYSTEM.to_owned()).push(" AND (i.version IS NULL OR s.version=i.version) AND (i.ancestor IS NULL OR EXISTS(SELECT 1 FROM bounded WHERE code=i.ancestor))), ")
        .push("charged AS (SELECT m.*,768+8::bigint*(octet_length(").push_bind(snomed::SYSTEM.to_owned())
        .push("::text)+COALESCE(octet_length(version),0)+octet_length(code)+COALESCE(octet_length(display),0))>")
        .push(PAGE_BYTES.to_string()).push(" AS exceeded FROM matched m) ")
        .push(format!("SELECT s.native_provider,s.anchor_known,(SELECT count(*) FROM bounded)>{MEMBERSHIP_WALK} AS walk_exceeded,COALESCE(m.exceeded,false) AS page_exceeded,CASE WHEN NOT m.exceeded THEN m.version END AS version,CASE WHEN NOT m.exceeded THEN m.code END AS code,CASE WHEN NOT m.exceeded THEN m.display END AS display,CASE WHEN NOT m.exceeded THEN COALESCE(m.properties,'[]'::jsonb) @> '[{{\"code\":\"inactive\",\"valueBoolean\":true}}]'::jsonb ELSE false END AS inactive FROM context s LEFT JOIN charged m ON true"));
    let row: Row = sql
        .build_query_as()
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "SNOMED release {release:?} not found (implicit default: International)"
            ))
        })?;
    if row.native_provider.as_deref() != Some("snomed-rf2") {
        return Err(unsupported("Native composition requires an RF2 projection"));
    }
    if !row.anchor_known {
        return Err(unsupported(format!(
            "Unknown native hierarchy/property code {}",
            definition.ancestor.as_deref().unwrap_or_default()
        )));
    }
    if row.page_exceeded {
        return Err(unsupported(
            "Native response page exceeds 32 MiB budget; request fewer entries",
        ));
    }
    // Do not bypass the full guard for wrong-system, wrong-version or early matches.
    if row.walk_exceeded {
        return Err(unsupported(
            "Native hierarchy traversal limit exceeded; no partial result returned",
        ));
    }
    let member = row.code.map(|code| Member {
        system: snomed::SYSTEM.into(),
        version: row.version,
        code,
        display: row.display,
        inactive: row.inactive,
        case_sensitive: true,
        abstract_code: false,
    });
    Ok(ValueSetResult::Native(ValueSetPage {
        sources: Default::default(),
        resource: definition.resource(root),
        total: i64::from(member.is_some()),
        members: member.into_iter().collect(),
        finite: None,
    }))
}
