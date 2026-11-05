//! Shared transaction-scoped compose loading for cached and multi-code operations.
use super::{inline::InlineValueSet, ValueSetRequest};
use crate::{
    error::AppError,
    models::{ExpandedValueSet, ExpansionSources},
};
use serde::Deserialize;
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use term_squid_core::{Canonical, Compose, PreparedSystem, Snapshot, MAX_RESOURCES};

pub(super) struct Loaded {
    snapshot: Snapshot,
    shared: term_squid_core::SharedSystems,
    pub(super) raw_systems: BTreeMap<Canonical, serde_json::Value>,
    supplements: BTreeMap<Canonical, std::sync::Arc<Vec<serde_json::Value>>>,
    root: Canonical,
    base_resource: Option<serde_json::Value>,
    sources: ExpansionSources,
    ucum: super::ucum_compose::Projection,
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
    let mut shared = term_squid_core::SharedSystems::new();
    let mut raw_systems = std::collections::BTreeMap::new();
    let mut overlays = BTreeMap::new();
    let mut ucum = super::ucum_compose::Projection::default();
    let bytes_remaining = budget;
    let mut pending = vec![root.clone()];
    let mut seen = BTreeSet::new();
    let mut base_resource = None;
    let mut sources = ExpansionSources::default();
    let mut requirements = super::supplements::Requirements::default();
    let mut contained = super::contained::ContainedSets::default();
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
        let local = contained.is_local(&key);
        let mut resource = if let Some(resource) = contained.take(&key) {
            super::options::check_resource_date(
                &resource,
                options.and_then(|o| o.date.as_deref()),
            )?;
            resource
        } else if key == root && inline.is_some() {
            let inline = inline.take().expect("Inline root is present");
            super::options::check_resource_date(
                &inline.resource,
                options.and_then(|o| o.date.as_deref()),
            )?;
            *bytes_remaining -= inline.bytes as i64;
            inline.resource
        } else if let Some(entry) = super::transient::select(
            super::transient::Kind::Set,
            &key.url,
            key.version.as_deref(),
            if key == root { id } else { None },
            options.and_then(|o| o.date.as_deref()),
        )? {
            *bytes_remaining -= entry.bytes as i64;
            (*entry.resource).clone()
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
        requirements.collect(&resource)?;
        if !local {
            contained.register(&key, &mut resource)?;
        }
        if let Some(options) = options {
            options.compose(tx, &mut resource).await?;
        }
        if key != root && !local {
            sources
                .value_sets
                .insert(Canonical::parse(&key.url, resource["version"].as_str())?);
        }
        let compose: Compose =
            serde_json::from_value(resource.get("compose").cloned().ok_or_else(|| {
                AppError::BadRequest("ValueSet without compose is not supported".into())
            })?)
            .map_err(|e| AppError::BadRequest(format!("Invalid compose: {e}")))?;
        ucum.collect(&compose, system_context)?;
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
            if ucum.keys.contains(&system) {
                // Delay grammar evaluation until the existing bounded CPU preparation.
                raw_systems.insert(system, serde_json::Value::Null);
                continue;
            }
            if let Some(entry) = super::transient::select(
                super::transient::Kind::System,
                &system.url,
                system.version.as_deref().or_else(|| {
                    system_context
                        .filter(|(url, _)| *url == system.url)
                        .map(|(_, version)| version)
                }),
                None,
                options.and_then(|o| o.date.as_deref()),
            )? {
                *bytes_remaining -= entry.bytes as i64;
                if *bytes_remaining < 0 {
                    return Err(AppError::BadRequest("Compose input exceeds 32 MiB".into()));
                }
                if let Some(prepared) = &entry.system {
                    shared.insert(system.clone(), prepared.clone());
                }
                raw_systems.insert(system, (*entry.resource).clone());
                sources
                    .supplements
                    .extend(entry.supplements.iter().cloned());
                continue;
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
                for supplement in &supplements {
                    sources.supplements.insert(Canonical::parse(
                        supplement["url"].as_str().expect("Stored supplement URL"),
                        supplement["version"].as_str(),
                    )?);
                }
                overlays.insert(system.clone(), std::sync::Arc::new(supplements));
            }
            raw_systems.insert(system, resource);
        }
        snapshot.value_sets.insert(key, compose);
    }
    if !ucum.keys.is_empty() {
        let resource = ucum.resource(options.and_then(|o| o.date.as_deref()))?;
        let size = serde_json::to_vec(&resource)
            .map_err(|e| AppError::Internal(e.into()))?
            .len() as i64;
        let supplements = super::supplements::load(
            tx,
            term_squid_core::ucum::SYSTEM,
            Some(term_squid_core::ucum::VERSION),
            options.and_then(|o| o.date.as_deref()),
            bytes_remaining,
        )
        .await?;
        for supplement in &supplements {
            sources.supplements.insert(Canonical::parse(
                supplement["url"].as_str().expect("Stored supplement URL"),
                supplement["version"].as_str(),
            )?);
        }
        let supplements = std::sync::Arc::new(supplements);
        for key in &ucum.keys {
            *bytes_remaining -= size;
            if *bytes_remaining < 0 {
                return Err(AppError::BadRequest("Compose input exceeds 32 MiB".into()));
            }
            raw_systems.insert(key.clone(), resource.clone());
            if !supplements.is_empty() {
                overlays.insert(key.clone(), supplements.clone());
            }
        }
    }
    requirements.check(&sources)?;
    Ok(Loaded {
        snapshot,
        shared,
        raw_systems,
        supplements: overlays,
        root,
        base_resource,
        sources,
        ucum,
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
        shared,
        mut raw_systems,
        supplements,
        root,
        base_resource,
        mut sources,
        ucum,
    } = loaded;
    let mut concept_count = 0;
    for (key, resource) in &mut raw_systems {
        if let Some(prepared) = shared.get(key) {
            // Request-local preparation already validated types and the complete graph.
            // The core shared entry point still enforces composition/lifecycle policy.
            sources.systems.insert(prepared.canonical.clone());
            concept_count += prepared.concepts.len();
            if concept_count > term_squid_core::MAX_CONCEPTS {
                return Err(AppError::BadRequest(
                    "Snapshot concept limit exceeded".into(),
                ));
            }
            continue;
        }
        if let Some(overlays) = supplements.get(key) {
            let targeted;
            let overlays: &[serde_json::Value] = if ucum.keys.contains(key) {
                let codes = resource["concept"]
                    .as_array()
                    .expect("Bundled projection concepts")
                    .iter()
                    .map(|c| c["code"].as_str().expect("Projection code").to_owned())
                    .collect();
                targeted = overlays
                    .iter()
                    .map(|overlay| super::supplements::for_codes(overlay, &codes, true))
                    .collect::<Result<Vec<_>, _>>()?;
                &targeted
            } else {
                overlays
            };
            *resource = term_squid_core::supplements::apply(resource, overlays)?;
        }
        let definition = term_squid_core::CodeSystem::deserialize(&*resource)
            .map_err(|e| AppError::BadRequest(format!("Invalid CodeSystem: {e}")))?;
        let prepared = if ucum.keys.contains(key) {
            let prepared = term_squid_core::ucum::UcumProvider::prepare_explicit(definition)?;
            for concept in resource["concept"]
                .as_array_mut()
                .expect("Bundled projection concepts")
            {
                let display = prepared
                    .concept(concept["code"].as_str().expect("Projection code"))
                    .and_then(|c| c.display.as_ref())
                    .expect("Grammar display");
                concept["display"] = display.clone().into();
            }
            prepared
        } else {
            PreparedSystem::prepare_for_r4(definition)?
        };
        sources.systems.insert(prepared.canonical.clone());
        concept_count += prepared.concepts.len();
        if concept_count > term_squid_core::MAX_CONCEPTS {
            return Err(AppError::BadRequest(
                "Snapshot concept limit exceeded".into(),
            ));
        }
        snapshot.systems.insert(key.clone(), prepared);
    }
    let members =
        term_squid_core::PreparedValueSet::prepare_with_shared(&snapshot, &root, &shared)?;
    let knowledge = snapshot
        .systems
        .into_values()
        .map(|system| {
            let index = system.into_code_index();
            (index.canonical.clone(), index)
        })
        .chain(shared.values().map(|system| {
            let index = system.code_index();
            (index.canonical.clone(), index)
        }))
        .collect();
    let result = ExpandedValueSet {
        resource: std::sync::Arc::new(base_resource.expect("Root is resolved")),
        members: std::sync::Arc::new(members),
        sources: std::sync::Arc::new(sources),
        knowledge: std::sync::Arc::new(knowledge),
    };
    Ok((result, raw_systems))
}
