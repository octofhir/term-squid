//! Shared transaction-scoped compose loading for cached and multi-code operations.
use super::{inline::InlineValueSet, ValueSetRequest};
use crate::{error::AppError, models::ExpandedValueSet};
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use term_squid_core::{Canonical, Compose, PreparedSystem, Snapshot, MAX_RESOURCES};

pub(super) struct Loaded {
    snapshot: Snapshot,
    pub(super) raw_systems: BTreeMap<Canonical, serde_json::Value>,
    supplements: BTreeMap<Canonical, Vec<serde_json::Value>>,
    root: Canonical,
    base_resource: Option<serde_json::Value>,
}
pub(super) async fn load(
    tx: &mut Transaction<'_, Postgres>,
    request: ValueSetRequest<'_>,
    budget: &mut i64,
) -> Result<Loaded, AppError> {
    load_options(tx, request, budget, None).await
}
pub(super) async fn load_options(
    tx: &mut Transaction<'_, Postgres>,
    request: ValueSetRequest<'_>,
    budget: &mut i64,
    options: Option<&super::ExpansionOptions>,
) -> Result<Loaded, AppError> {
    let root = request
        .inline
        .map(|r| InlineValueSet::new(r, request.version))
        .transpose()?;
    let canonical = root
        .as_ref()
        .map(|r| r.canonical.clone())
        .map(Ok)
        .unwrap_or_else(|| Canonical::parse(request.url, request.version))?;
    load_root_options(
        tx,
        canonical,
        request.id,
        request.system_context,
        root,
        budget,
        options,
    )
    .await
}
pub(super) async fn load_root(
    tx: &mut Transaction<'_, Postgres>,
    root: Canonical,
    id: Option<uuid::Uuid>,
    system_context: Option<(&str, &str)>,
    inline: Option<InlineValueSet>,
    budget: &mut i64,
) -> Result<Loaded, AppError> {
    load_root_options(tx, root, id, system_context, inline, budget, None).await
}
async fn load_root_options(
    tx: &mut Transaction<'_, Postgres>,
    root: Canonical,
    id: Option<uuid::Uuid>,
    system_context: Option<(&str, &str)>,
    inline: Option<InlineValueSet>,
    budget: &mut i64,
    options: Option<&super::ExpansionOptions>,
) -> Result<Loaded, AppError> {
    let mut snapshot = Snapshot::default();
    let mut raw_systems = std::collections::BTreeMap::new();
    let mut overlays = BTreeMap::new();
    let bytes_remaining = budget;
    let mut pending = vec![root.clone()];
    let mut seen = BTreeSet::new();
    let mut base_resource = None;
    let mut inline = inline;
    while let Some(key) = pending.pop() {
        if !seen.insert(key.clone()) {
            continue;
        }
        if seen.len() + raw_systems.len() > MAX_RESOURCES {
            return Err(AppError::BadRequest(
                "Compose resource limit exceeded".into(),
            ));
        }
        if term_squid_core::snomed::implicit(&key)?.is_some()
            || term_squid_core::loinc::implicit(&key)?.is_some()
        {
            return Err(AppError::NativeValueSet);
        }
        let mut resource = if key == root && inline.is_some() {
            let inline = inline.take().expect("Inline root is present");
            super::options::check_resource_date(
                &inline.resource,
                options.and_then(|o| o.date.as_deref()),
            )?;
            *bytes_remaining -= inline.bytes as i64;
            inline.resource
        } else if let Some(options) = options.filter(|o| o.date.is_some()) {
            super::selection::set(
                tx,
                &key,
                if key == root { id } else { None },
                options.date.as_deref(),
                bytes_remaining,
            )
            .await?
        } else {
            let row: Option<(Option<sqlx::types::Json<serde_json::Value>>, i64)> = sqlx::query_as(
                "SELECT CASE WHEN octet_length(content::text) <= $4 THEN content END, octet_length(content::text)::bigint FROM value_sets WHERE ($3::uuid IS NOT NULL AND id = $3) OR ($3::uuid IS NULL AND url = $1 AND ($2::text IS NULL OR version = $2)) ORDER BY updated_at DESC, id LIMIT 1")
                .bind(&key.url).bind(&key.version).bind(if key == root { id } else { None }).bind(*bytes_remaining).fetch_optional(&mut **tx).await?;
            let row = row.ok_or_else(|| {
                AppError::NotFound(format!(
                    "ValueSet {} version {:?} not found",
                    key.url, key.version
                ))
            })?;
            *bytes_remaining -= row.1;
            row.0
                .ok_or_else(|| AppError::BadRequest("Compose input exceeds 32 MiB".into()))?
                .0
        };
        if key == root {
            base_resource = Some(resource.clone());
        }
        if let Some(options) = options {
            options.compose(tx, &mut resource).await?;
        }
        let compose: Compose =
            serde_json::from_value(resource.get("compose").cloned().ok_or_else(|| {
                AppError::BadRequest("ValueSet without compose is not supported".into())
            })?)
            .map_err(|e| AppError::BadRequest(format!("Invalid compose: {e}")))?;
        let (systems, imports) = compose.references()?;
        pending.extend(imports);
        for system in systems {
            if raw_systems.contains_key(&system) {
                continue;
            }
            if seen.len() + raw_systems.len() >= MAX_RESOURCES {
                return Err(AppError::BadRequest(
                    "Compose resource limit exceeded".into(),
                ));
            }
            let row: Option<(Option<sqlx::types::Json<serde_json::Value>>, i64, Option<String>)> = sqlx::query_as(
                    "SELECT CASE WHEN native_provider IS NULL AND octet_length(content::text) <= $3 THEN content END, octet_length(content::text)::bigint, native_provider FROM code_systems WHERE url = $1 AND ($2::text IS NULL OR version = $2) ORDER BY updated_at DESC, id LIMIT 1")
                    .bind(&system.url).bind(system.version.as_deref().or_else(|| system_context.filter(|(url, _)| *url == system.url).map(|(_, version)| version))).bind(*bytes_remaining).fetch_optional(&mut **tx).await?;
            let row = row.ok_or_else(|| {
                AppError::NotFound(format!(
                    "CodeSystem {} version {:?} not found",
                    system.url, system.version
                ))
            })?;
            if row.2.is_some() {
                return Err(AppError::NativeValueSet);
            }
            *bytes_remaining -= row.1;
            let resource = row
                .0
                .ok_or_else(|| AppError::BadRequest("Compose input exceeds 32 MiB".into()))?
                .0;
            let supplements = super::supplements::load(
                tx,
                &system.url,
                resource["version"].as_str(),
                options.and_then(|o| o.date.as_deref()),
                bytes_remaining,
            )
            .await?;
            if !supplements.is_empty() {
                overlays.insert(system.clone(), supplements);
            }
            raw_systems.insert(system, resource);
        }
        snapshot.value_sets.insert(key, compose);
    }
    Ok(Loaded {
        snapshot,
        raw_systems,
        supplements: overlays,
        root,
        base_resource,
    })
}
pub(super) fn prepare(loaded: Loaded) -> Result<ExpandedValueSet, AppError> {
    prepare_rich(loaded).map(|(expanded, _)| expanded)
}
pub(super) fn prepare_rich(
    loaded: Loaded,
) -> Result<(ExpandedValueSet, BTreeMap<Canonical, serde_json::Value>), AppError> {
    let Loaded {
        mut snapshot,
        mut raw_systems,
        supplements,
        root,
        base_resource,
    } = loaded;
    let mut concept_count = 0;
    for (key, resource) in &mut raw_systems {
        if let Some(overlays) = supplements.get(key) {
            *resource = term_squid_core::supplements::apply(resource, overlays)?;
        }
        let definition = serde_json::from_value(resource.clone())
            .map_err(|e| AppError::BadRequest(format!("Invalid CodeSystem: {e}")))?;
        let prepared = PreparedSystem::prepare(definition)?;
        concept_count += prepared.concepts.len();
        if concept_count > term_squid_core::MAX_CONCEPTS {
            return Err(AppError::BadRequest(
                "Snapshot concept limit exceeded".into(),
            ));
        }
        snapshot.systems.insert(key.clone(), prepared);
    }
    let result = ExpandedValueSet {
        resource: std::sync::Arc::new(base_resource.expect("Root is resolved")),
        members: std::sync::Arc::new(term_squid_core::PreparedValueSet::prepare(
            &snapshot, &root,
        )?),
    };
    Ok((result, raw_systems))
}
