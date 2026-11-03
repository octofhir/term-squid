//! One atomic statement for finite closures and bounded native RF2 ancestor walks.
use crate::{error::AppError, models::ResolvedSubsumption};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct Row {
    native_provider: Option<String>,
    hierarchy_meaning: Option<String>,
    code_a: Option<String>,
    code_b: Option<String>,
    relationship: Option<bool>,
    native_limited: bool,
}

pub(super) async fn resolve(
    pool: &PgPool,
    url: &str,
    version: Option<&str>,
    code_a: &str,
    code_b: &str,
    id: Option<Uuid>,
) -> Result<Option<ResolvedSubsumption>, AppError> {
    let row = sqlx::query_as::<_, Row>(include_str!("subsumption.sql"))
        .bind(url)
        .bind(version)
        .bind(code_a)
        .bind(code_b)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.map(|row| {
        if row.native_provider.as_deref() == Some("snomed-rf2") {
            term_squid_core::snomed::reject_expression(code_a)?;
            term_squid_core::snomed::reject_expression(code_b)?;
        }
        if row.native_limited {
            return Err(AppError::Terminology(term_squid_core::Error(
                "Native SNOMED ancestor traversal exceeds 8192 visited entries".into(),
            )));
        }
        Ok(ResolvedSubsumption {
            hierarchy_meaning: row.hierarchy_meaning,
            code_a: row.code_a,
            code_b: row.code_b,
            relationship: row.relationship,
        })
    })
    .transpose()
}
