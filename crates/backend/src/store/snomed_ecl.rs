//! Active-substrate ECL predicates over bounded release-scoped RF2 edge walks.
//! SQL identifiers come only from the closed AST and generated CTE names.
use super::native_valueset::unsupported;
use crate::error::AppError;
use sqlx::{Postgres, QueryBuilder, Transaction};
use term_squid_core::snomed::{ecl::Expression, Ecl};
use uuid::Uuid;

const ACTIVE: &str = "NOT(COALESCE(a.properties,'[]'::jsonb) @> '[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb)";

pub(super) async fn validate_refsets(
    tx: &mut Transaction<'_, Postgres>,
    system: Uuid,
    refsets: &std::collections::BTreeSet<&str>,
) -> Result<(), AppError> {
    if refsets.is_empty() {
        return Ok(());
    }
    let ids: Vec<_> = refsets.iter().copied().collect();
    let (loaded,known):(bool,i64)=sqlx::query_as("SELECT r.simple_refsets_loaded,(SELECT count(*) FROM snomed_simple_refsets f JOIN concepts c ON c.code_system_id=f.code_system_id AND c.code=f.refset_id WHERE f.code_system_id=r.code_system_id AND f.refset_id=ANY($2::text[]) AND r.simple_refset_source->'refsetIds' ? f.refset_id AND NOT(COALESCE(c.properties,'[]'::jsonb) @> '[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb)) FROM snomed_releases r WHERE r.code_system_id=$1")
        .bind(system).bind(&ids).fetch_one(&mut **tx).await?;
    if !loaded || known != ids.len() as i64 {
        return Err(unsupported(
            "SNOMED concept refset source is unavailable or undeclared for the selected release",
        ));
    }
    Ok(())
}

pub(super) async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    system: Uuid,
    ecl: &Ecl,
) -> Result<(), AppError> {
    validate_refsets(tx, system, &ecl.refsets()).await?;
    let anchors: Vec<_> = ecl.anchors().into_iter().collect();
    if anchors.is_empty() {
        return Ok(());
    }
    let known: i64 = sqlx::query_scalar("SELECT count(*) FROM concepts a WHERE a.code_system_id=$1 AND a.code=ANY($2::text[]) AND NOT(COALESCE(a.properties,'[]'::jsonb) @> '[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb)")
        .bind(system).bind(&anchors).fetch_one(&mut **tx).await?;
    if known != anchors.len() as i64 {
        return Err(unsupported(
            "Unknown or inactive ECL focus concept in the selected release",
        ));
    }
    Ok(())
}

pub(super) struct Prepared {
    walks: Vec<String>,
}

pub(super) fn prepare(
    sql: &mut QueryBuilder<Postgres>,
    serial: &mut usize,
    guards: &mut Vec<(String, i64)>,
    system: Uuid,
    ecl: &Ecl,
    membership: Option<&str>,
    limit: i64,
) -> Prepared {
    let mut walks = Vec::new();
    for (operator, anchor) in ecl.hierarchies() {
        *serial += 1;
        let name = format!("p{serial}");
        let start = if !operator.ancestors() {
            membership.unwrap_or(anchor)
        } else {
            anchor
        };
        let ascending = operator.ancestors() || membership.is_some();
        let (source, destination) = if ascending {
            ("source_code", "destination_code")
        } else {
            ("destination_code", "source_code")
        };
        sql.push(format!(
            "{name}_walk(code) AS (SELECT a.code::text FROM concepts a WHERE a.code_system_id="
        ))
        .push_bind(system)
        .push(" AND a.code=")
        .push_bind(start.to_owned())
        .push(" AND ")
        .push(ACTIVE)
        .push(format!(" UNION SELECT r.{destination}::text FROM "));
        if operator.direct() {
            sql.push("snomed_relationships r WHERE r.code_system_id=")
                .push_bind(system)
                .push(format!(" AND r.{source}="))
                .push_bind(start.to_owned());
        } else {
            sql.push(format!("{name}_walk w CROSS JOIN LATERAL (SELECT {destination} FROM snomed_relationships r WHERE r.{source}=w.code AND r.code_system_id="))
                .push_bind(system).push(" AND r.active AND r.type_id='116680003' AND r.characteristic_type_id='900000000000011006' OFFSET 0) r WHERE true");
        }
        if operator.direct() {
            sql.push(" AND r.active AND r.type_id='116680003' AND r.characteristic_type_id='900000000000011006'");
            // A membership seed can itself be inactive or unknown. Do not cross an
            // inactive source even if malformed direct SQL added an active edge.
            sql.push(" AND EXISTS(SELECT 1 FROM concepts a WHERE a.code_system_id=")
                .push_bind(system)
                .push(" AND a.code=")
                .push_bind(start.to_owned())
                .push(" AND ")
                .push(ACTIVE)
                .push(")");
        }
        sql.push(" AND EXISTS(SELECT 1 FROM concepts a WHERE a.code_system_id=").push_bind(system)
            .push(format!(" AND a.code=r.{destination} AND ")).push(ACTIVE).push(")" )
            .push(format!("), {name} AS MATERIALIZED (SELECT code FROM {name}_walk LIMIT {}), {name}_size AS MATERIALIZED (SELECT count(*)::bigint AS n FROM {name}), ",limit+1));
        guards.push((name.clone(), limit));
        walks.push(name);
    }
    Prepared { walks }
}

pub(super) fn predicate(
    sql: &mut QueryBuilder<Postgres>,
    ecl: &Ecl,
    prepared: &Prepared,
    membership: bool,
) {
    fn expression(
        sql: &mut QueryBuilder<Postgres>,
        node: &Expression,
        walks: &mut std::slice::Iter<'_, String>,
        membership: bool,
    ) {
        sql.push("(");
        match node {
            Expression::All => {
                sql.push("true");
            }
            Expression::Concept(code) => {
                sql.push("c.code=").push_bind(code.clone());
            }
            Expression::MemberOf(refset) => {
                sql.push("EXISTS(SELECT 1 FROM snomed_simple_members m WHERE m.code_system_id=c.code_system_id AND m.refset_id=")
                    .push_bind(refset.clone()).push(" AND m.referenced_component_id=c.code AND m.active)");
            }
            Expression::Hierarchy { operator, code } => {
                let walk = walks
                    .next()
                    .expect("Each hierarchy AST leaf has a guarded walk");
                if membership && !operator.ancestors() {
                    sql.push(format!("EXISTS(SELECT 1 FROM {walk} WHERE code="))
                        .push_bind(code.clone())
                        .push(")");
                } else {
                    sql.push(format!("c.code IN (SELECT code FROM {walk})"));
                }
                if !operator.include_self() {
                    sql.push(" AND c.code<>").push_bind(code.clone());
                }
            }
            Expression::And(children) | Expression::Or(children) => {
                for (index, child) in children.iter().enumerate() {
                    if index != 0 {
                        sql.push(if matches!(node, Expression::And(_)) {
                            " AND "
                        } else {
                            " OR "
                        });
                    }
                    expression(sql, child, walks, membership);
                }
            }
            Expression::Minus(left, right) => {
                expression(sql, left, walks, membership);
                sql.push(" AND NOT ");
                expression(sql, right, walks, membership);
            }
        }
        sql.push(")");
    }
    sql.push("NOT(COALESCE(c.properties,'[]'::jsonb) @> '[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb) AND ");
    expression(sql, ecl.root(), &mut prepared.walks.iter(), membership);
}
