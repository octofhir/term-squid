//! Local resource selection for optional historical and binding-context workflows.
// Dynamic SQL interpolates only closed table/fragment inventories; external values are bound.
use super::options;
use crate::error::AppError;
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use term_squid_core::Canonical;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) enum Family {
    System,
    Set,
}
impl Family {
    fn table(self) -> &'static str {
        match self {
            Self::System => "code_systems",
            Self::Set => "value_sets",
        }
    }
}
pub(super) async fn id(
    tx: &mut Transaction<'_, Postgres>,
    family: Family,
    key: &Canonical,
    instance: Option<Uuid>,
    date: Option<&str>,
) -> Result<Uuid, AppError> {
    let sql=format!("SELECT id,content->>'date' FROM {} WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3 IS NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) ORDER BY updated_at DESC,id LIMIT {}",family.table(),if date.is_some(){129}else{1});
    let rows: Vec<(Uuid, Option<String>)> = sqlx::query_as(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(&key.url)
        .bind(&key.version)
        .bind(instance)
        .fetch_all(&mut **tx)
        .await?;
    if rows.len() > 128 {
        return Err(AppError::BadRequest(
            "Historical selection exceeds 128 versions; specify a version".into(),
        ));
    }
    if let Some(date) = date {
        let cutoff = options::date(date)?;
        let mut selected = None;
        for (id, source_date) in &rows {
            let source_date = if let Some(date) = source_date {
                Some(options::date(date)?)
            } else if matches!(family, Family::System) {
                let release: Option<String> = sqlx::query_scalar(
                    "SELECT release_date::text FROM snomed_releases WHERE code_system_id=$1",
                )
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?;
                release.as_deref().map(options::date).transpose()?
            } else {
                None
            };
            if let Some(source_date) = source_date.filter(|d| *d <= cutoff) {
                if selected.is_none_or(|(previous, _)| source_date > previous) {
                    selected = Some((source_date, *id));
                }
            } else if source_date.is_none() && (instance.is_some() || key.version.is_some()) {
                // An explicitly selected immutable edition is stable; no historical edition is inferred.
                return Ok(*id);
            }
        }
        return selected.map(|(_, id)| id).ok_or_else(|| {
            AppError::NotFound(
                "Historical resource definition is unavailable for the requested date".into(),
            )
        });
    }
    rows.first()
        .map(|r| r.0)
        .ok_or_else(|| AppError::NotFound(format!("{} {} not found", family.table(), key.url)))
}
pub(super) async fn set(
    tx: &mut Transaction<'_, Postgres>,
    key: &Canonical,
    instance: Option<Uuid>,
    date: Option<&str>,
    budget: &mut i64,
) -> Result<Value, AppError> {
    let id = id(tx, Family::Set, key, instance, date).await?;
    let (content,bytes):(Option<sqlx::types::Json<Value>>,i64)=sqlx::query_as("SELECT CASE WHEN octet_length(content::text)<=$2 THEN content END,octet_length(content::text)::bigint FROM value_sets WHERE id=$1").bind(id).bind(*budget).fetch_one(&mut **tx).await?;
    *budget -= bytes;
    content
        .map(|r| r.0)
        .ok_or_else(|| AppError::BadRequest("Compose source byte limit exceeded".into()))
}

/// The standalone registry supports local ValueSet canonicals and logical instance references.
/// No context URI is dereferenced over the network or interpreted as a filesystem path.
pub(super) fn context(value: &str) -> Result<(Canonical, Option<Uuid>), AppError> {
    if let Some(id) = value.strip_prefix("ValueSet/") {
        return Ok((
            Canonical::parse("urn:term-squid:instance", None)?,
            Some(
                id.parse().map_err(|_| {
                    AppError::BadRequest("context ValueSet id must be a UUID".into())
                })?,
            ),
        ));
    }
    if value.contains('#') {
        return Err(AppError::BadRequest("StructureDefinition binding contexts require a conformance registry; use a local ValueSet canonical or ValueSet/{uuid}".into()));
    }
    Ok((Canonical::parse(value, None)?, None))
}

pub(super) async fn infer_system(
    tx: &mut Transaction<'_, Postgres>,
    request: super::ValueSetRequest<'_>,
    date: Option<&str>,
) -> Result<String, AppError> {
    let root = Canonical::parse(request.url, request.version)?;
    let mut pending = vec![root.clone()];
    let mut seen = std::collections::BTreeSet::new();
    let mut systems = std::collections::BTreeSet::new();
    let mut budget = 4 * 1024 * 1024;
    while let Some(key) = pending.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        if seen.len() > 128 {
            return Err(AppError::BadRequest(
                "ValueSet context dependency limit exceeded".into(),
            ));
        }
        let implicit =
            term_squid_core::snomed::implicit(&key)?.or(term_squid_core::loinc::implicit(&key)?);
        let resource = if let Some(resource) = request.inline.filter(|_| key == root) {
            resource.clone()
        } else if let Some(implicit) = implicit {
            implicit
        } else {
            set(
                tx,
                &key,
                if key == root { request.id } else { None },
                date,
                &mut budget,
            )
            .await?
        };
        let compose: term_squid_core::Compose = serde_json::from_value(resource["compose"].clone())
            .map_err(|e| AppError::BadRequest(format!("Invalid compose: {e}")))?;
        for selection in compose.include {
            if let Some(system) = selection.system {
                systems.insert(Canonical::parse(&system, selection.version.as_deref())?.url);
            }
            for reference in selection.value_set {
                pending.push(Canonical::parse(&reference, None)?);
            }
        }
    }
    if systems.len() != 1 {
        return Err(AppError::BadRequest(
            "system is required when the ValueSet does not identify exactly one CodeSystem".into(),
        ));
    }
    Ok(systems.into_iter().next().expect("One system"))
}
