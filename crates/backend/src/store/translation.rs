//! Bounded map discovery, chained maps and scope membership share one reader snapshot.
use super::{compose_loader, native_valueset, ValueSetOperation, ValueSetRequest, ValueSetResult};
use crate::error::AppError;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use term_squid_core::{
    Canonical, PreparedConceptMap, ResolvedTranslation, TranslationContinuation,
    TranslationDependency,
};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

pub struct TranslationCode<'a> {
    pub system: Option<&'a str>,
    pub version: Option<&'a str>,
    pub code: &'a str,
}
pub struct TranslationRequest<'a> {
    pub url: Option<&'a str>,
    pub version: Option<&'a str>,
    pub id: Option<Uuid>,
    pub inline: Option<&'a Value>,
    pub source: Option<&'a str>,
    pub target: Option<&'a str>,
    pub target_system: Option<&'a str>,
    pub reverse: bool,
    pub codes: &'a [TranslationCode<'a>],
    pub dependencies: &'a [TranslationDependency<'a>],
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TranslationAnswer {
    pub concept: ResolvedTranslation,
    pub source: Option<String>,
}
type Tx<'a> = Transaction<'a, Postgres>;
type MapRow = (
    String,
    Option<String>,
    Option<sqlx::types::Json<Value>>,
    i64,
);

pub(super) async fn run(
    pool: &PgPool,
    request: TranslationRequest<'_>,
    permit: OwnedSemaphorePermit,
) -> Result<Vec<TranslationAnswer>, AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    let inferred = if request.codes.iter().any(|c| c.system.is_none()) {
        if let Some(source) = request.source {
            let (canonical, id) = super::selection::context(source)?;
            Some(
                super::selection::infer_system(
                    &mut tx,
                    ValueSetRequest {
                        url: &canonical.url,
                        version: canonical.version.as_deref(),
                        id,
                        inline: None,
                        system_context: None,
                    },
                    None,
                )
                .await?,
            )
        } else if request.url.is_none() && request.id.is_none() && request.inline.is_none() {
            return Err(AppError::BadRequest(
                "Map discovery without system requires an unambiguous source ValueSet".into(),
            ));
        } else {
            None
        }
    } else {
        None
    };
    let mut budget = 32 * 1024 * 1024;
    let mut roots = Vec::new();
    let mut raw = BTreeMap::new();
    let mut pending = Vec::new();
    let temporary_maps = if request.id.is_none() && request.inline.is_none() {
        if let Some(url) = request.url {
            super::transient::select(
                super::transient::Kind::Map,
                url,
                request.version,
                None,
                None,
            )?
            .into_iter()
            .collect::<Vec<_>>()
        } else {
            super::transient::current()
                .map(|context| context.maps(request.version))
                .transpose()?
                .unwrap_or_default()
        }
    } else {
        Vec::new()
    };
    let rows: Vec<MapRow> = if let Some(resource) = request.inline {
        let key = Canonical::parse(
            resource["url"]
                .as_str()
                .unwrap_or("urn:term-squid:inline-map"),
            resource["version"].as_str(),
        )?;
        roots.push(key.clone());
        pending.extend(links(resource)?);
        budget -= serde_json::to_vec(resource)
            .map_err(|e| AppError::Internal(e.into()))?
            .len() as i64;
        raw.insert(key, resource.clone());
        Vec::new()
    } else if request.url.is_some() && !temporary_maps.is_empty() {
        Vec::new()
    } else {
        let canonical = request
            .url
            .map(|u| Canonical::parse(u, request.version))
            .transpose()?;
        let systems: Vec<_> = request
            .codes
            .iter()
            .filter_map(|c| c.system.or(inferred.as_deref()))
            .collect();
        sqlx::query_as("SELECT DISTINCT ON (url) url,version,CASE WHEN octet_length(content::text)<=$7 THEN content END,octet_length(content::text)::bigint FROM concept_maps WHERE ($3::uuid IS NOT NULL AND id=$3) OR ($3 IS NULL AND (($1::text IS NOT NULL AND url=$1 AND ($2::text IS NULL OR version=$2)) OR ($1 IS NULL AND EXISTS (SELECT 1 FROM jsonb_array_elements(COALESCE(content->'group','[]')) g WHERE g->>CASE WHEN $4 THEN 'target' ELSE 'source' END=ANY($5::text[])) AND ($6::text IS NULL OR version=$6)))) ORDER BY url,updated_at DESC,id LIMIT 129")
            .bind(canonical.as_ref().map(|c| c.url.as_str())).bind(canonical.as_ref().and_then(|c| c.version.as_deref())).bind(request.id).bind(request.reverse).bind(systems).bind(request.version).bind(budget).fetch_all(&mut *tx).await?
    };
    let mut rows = rows;
    rows.retain(|(url, _, _, _)| {
        !temporary_maps
            .iter()
            .any(|entry| entry.resource["url"] == url.as_str())
    });
    for entry in temporary_maps {
        let matches = request.url.is_some()
            || entry.resource["group"].as_array().is_some_and(|groups| {
                groups.iter().any(|group| {
                    group[if request.reverse { "target" } else { "source" }]
                        .as_str()
                        .is_some_and(|system| {
                            request
                                .codes
                                .iter()
                                .any(|code| code.system.or(inferred.as_deref()) == Some(system))
                        })
                })
            });
        if matches {
            rows.push((
                entry.resource["url"].as_str().unwrap().into(),
                entry.resource["version"].as_str().map(str::to_owned),
                Some(sqlx::types::Json((*entry.resource).clone())),
                entry.bytes as i64,
            ));
        }
    }
    if rows.len() > 128 {
        return Err(AppError::BadRequest(
            "ConceptMap discovery exceeds 128 maps; specify a URL".into(),
        ));
    }
    for (url, version, resource, bytes) in rows {
        let key = Canonical::parse(&url, version.as_deref())?;
        if request.id.is_some()
            && request
                .version
                .is_some_and(|v| Some(v) != key.version.as_deref())
        {
            return Err(AppError::BadRequest(
                "conceptMapVersion conflicts with instance".into(),
            ));
        }
        let mut resource = resource
            .ok_or_else(|| AppError::BadRequest("ConceptMap input exceeds 32 MiB".into()))?
            .0;
        resource["url"] = key.url.clone().into();
        if let Some(version) = &key.version {
            resource["version"] = version.clone().into();
        } else {
            resource
                .as_object_mut()
                .expect("ConceptMap object")
                .remove("version");
        }
        budget -= bytes;
        roots.push(key.clone());
        pending.extend(links(&resource)?);
        raw.insert(key, resource);
    }
    if roots.is_empty() && (request.url.is_some() || request.id.is_some()) {
        return Err(AppError::NotFound("ConceptMap not found".into()));
    }
    while let Some(key) = pending.pop() {
        if raw.contains_key(&key) {
            continue;
        }
        if raw.len() >= 128 || budget < 0 {
            return Err(AppError::BadRequest(
                "ConceptMap dependency budget exceeded".into(),
            ));
        }
        let row: Option<MapRow> = if let Some(entry) = super::transient::select(
            super::transient::Kind::Map,
            &key.url,
            key.version.as_deref(),
            None,
            None,
        )? {
            Some((
                entry.resource["url"].as_str().unwrap().into(),
                entry.resource["version"].as_str().map(str::to_owned),
                Some(sqlx::types::Json((*entry.resource).clone())),
                entry.bytes as i64,
            ))
        } else {
            sqlx::query_as("SELECT url,version,CASE WHEN octet_length(content::text)<=$3 THEN content END,octet_length(content::text)::bigint FROM concept_maps WHERE url=$1 AND ($2::text IS NULL OR version=$2) ORDER BY updated_at DESC,id LIMIT 1")
                .bind(&key.url).bind(&key.version).bind(budget).fetch_optional(&mut *tx).await?
        };
        let (url, version, resource, bytes) = row.ok_or_else(|| {
            AppError::NotFound(format!("Referenced ConceptMap {} not found", key.url))
        })?;
        budget -= bytes;
        let mut resource = resource
            .ok_or_else(|| AppError::BadRequest("ConceptMap input exceeds 32 MiB".into()))?
            .0;
        resource["url"] = url.into();
        if let Some(version) = version {
            resource["version"] = version.into();
        }
        pending.extend(links(&resource)?);
        raw.insert(key, resource);
    }
    if budget < 0 {
        return Err(AppError::BadRequest(
            "ConceptMap input exceeds 32 MiB".into(),
        ));
    }
    let (maps, permit) = tokio::task::spawn_blocking(move || {
        let result = raw
            .into_iter()
            .map(|(key, value)| {
                let plan = PreparedConceptMap::prepare(&value)?;
                Ok((key, plan))
            })
            .collect::<Result<BTreeMap<_, _>, term_squid_core::Error>>();
        (result, permit)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let maps = maps?;
    let mut scope = Scope {
        cache: BTreeMap::new(),
        budget,
        permit: Some(permit),
    };
    let mut result = BTreeSet::new();
    for root in roots {
        let plan = &maps[&root];
        for code in request.codes {
            let input_system = code
                .system
                .or(inferred.as_deref())
                .or_else(|| plan.input_system(request.reverse))
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "system is required for a ConceptMap with multiple input systems".into(),
                    )
                })?;
            if let Some(source) = request.source.filter(|s| {
                plan.source_scope(request.reverse)
                    .is_none_or(|actual| !same_scope(s, actual))
            }) {
                if !member(
                    &mut tx,
                    source,
                    input_system,
                    code.version,
                    code.code,
                    &mut scope,
                )
                .await?
                {
                    continue;
                }
            }
            let mut stack = vec![(
                root.clone(),
                BTreeSet::new(),
                Vec::<(Canonical, usize)>::new(),
            )];
            while let Some((key, mut path, fallback)) = stack.pop() {
                if path.len() >= 32 || !path.insert(key.clone()) {
                    return Err(AppError::BadRequest(
                        "Cyclic or excessively deep ConceptMap chain".into(),
                    ));
                }
                let plan = &maps[&key];
                let resolved = plan.translate_resolved(
                    input_system,
                    code.version,
                    code.code,
                    request.target_system,
                    request.reverse,
                    request.dependencies,
                )?;
                let mut matches = resolved.matches;
                for continuation in resolved.continuations {
                    match continuation {
                        TranslationContinuation::OtherMap { canonical, group } => {
                            stack.push((canonical, path.clone(), {
                                let mut constraints = fallback.clone();
                                if request.reverse {
                                    constraints.push((key.clone(), group));
                                }
                                constraints
                            }))
                        }
                        TranslationContinuation::FixedReverse {
                            system,
                            version,
                            group,
                        } => {
                            let rows = if let Some(entry) = super::transient::select(
                                super::transient::Kind::System,
                                &system,
                                version.as_deref(),
                                None,
                                None,
                            )? {
                                let prepared = entry.system.as_ref().filter(|_| entry.resource["content"] == "complete").ok_or_else(|| AppError::BadRequest("Reverse fixed mapping requires a complete known source CodeSystem".into()))?;
                                if prepared.concepts.len() > 10000 {
                                    return Err(AppError::BadRequest(
                                        "Reverse fixed mapping exceeds 10000 source codes".into(),
                                    ));
                                }
                                prepared
                                    .concepts
                                    .values()
                                    .map(|concept| {
                                        (
                                            concept.code.clone(),
                                            concept.display.clone(),
                                            prepared.canonical.version.clone(),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            } else {
                                let rows: Vec<(String,Option<String>,Option<String>)> = sqlx::query_as("SELECT c.code,c.display,s.version FROM (SELECT id,version,CASE WHEN native_provider IS NOT NULL THEN 'complete' ELSE content_mode END mode FROM code_systems WHERE url=$1 AND ($2::text IS NULL OR version=$2) ORDER BY updated_at DESC,id LIMIT 1) s JOIN concepts c ON c.code_system_id=s.id WHERE s.mode='complete' ORDER BY c.code LIMIT 10001")
                                    .bind(&system).bind(&version).fetch_all(&mut *tx).await?;
                                if rows.len() > 10000 {
                                    return Err(AppError::BadRequest(
                                        "Reverse fixed mapping exceeds 10000 source codes".into(),
                                    ));
                                }
                                let complete: Option<String> = sqlx::query_scalar("SELECT CASE WHEN native_provider IS NOT NULL THEN 'complete' ELSE content_mode END FROM code_systems WHERE url=$1 AND ($2::text IS NULL OR version=$2) ORDER BY updated_at DESC,id LIMIT 1").bind(&system).bind(&version).fetch_optional(&mut *tx).await?;
                                if complete.as_deref() != Some("complete") {
                                    return Err(AppError::BadRequest("Reverse fixed mapping requires a complete known source CodeSystem".into()));
                                }
                                rows
                            };
                            for (candidate, display, version) in rows {
                                if plan.fallback_accepts(
                                    group,
                                    &system,
                                    version.as_deref(),
                                    &candidate,
                                ) {
                                    matches.push(ResolvedTranslation {
                                        system: system.clone(),
                                        version,
                                        code: candidate,
                                        display,
                                        equivalence: "relatedto".into(),
                                        products: Vec::new(),
                                    });
                                }
                            }
                        }
                    }
                }
                for mapped in matches {
                    if fallback.iter().any(|(owner, group)| {
                        !maps[owner].fallback_accepts(
                            *group,
                            &mapped.system,
                            mapped.version.as_deref(),
                            &mapped.code,
                        )
                    }) {
                        continue;
                    }
                    if let Some(target) = request.target.filter(|s| {
                        maps[&root]
                            .target_scope(request.reverse)
                            .is_none_or(|actual| !same_scope(s, actual))
                    }) {
                        if !member(
                            &mut tx,
                            target,
                            &mapped.system,
                            mapped.version.as_deref(),
                            &mapped.code,
                            &mut scope,
                        )
                        .await?
                        {
                            continue;
                        }
                    }
                    result.insert(TranslationAnswer {
                        source: plan.url().map(|u| {
                            plan.version()
                                .map_or_else(|| u.to_owned(), |v| format!("{u}|{v}"))
                        }),
                        concept: mapped,
                    });
                    if result.len() > 10000 {
                        return Err(AppError::Overloaded);
                    }
                }
            }
        }
    }
    tx.commit().await?;
    drop(scope);
    Ok(result.into_iter().collect())
}
fn same_scope(a: &str, b: &str) -> bool {
    match (Canonical::parse(a, None), Canonical::parse(b, None)) {
        (Ok(a), Ok(b)) => a.url == b.url && a.version.zip(b.version).is_none_or(|(a, b)| a == b),
        _ => false,
    }
}
fn links(resource: &Value) -> Result<Vec<Canonical>, AppError> {
    resource["group"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| (g["unmapped"]["mode"] == "other-map").then_some(&g["unmapped"]["url"]))
        .map(|v| {
            Canonical::parse(
                v.as_str().ok_or_else(|| {
                    AppError::BadRequest("Unmapped ConceptMap URL required".into())
                })?,
                None,
            )
            .map_err(AppError::from)
        })
        .collect()
}
struct Scope {
    cache: BTreeMap<Canonical, Option<crate::models::ExpandedValueSet>>,
    budget: i64,
    permit: Option<OwnedSemaphorePermit>,
}
async fn member(
    tx: &mut Tx<'_>,
    url: &str,
    system: &str,
    version: Option<&str>,
    code: &str,
    scope: &mut Scope,
) -> Result<bool, AppError> {
    let (canonical, id) = super::selection::context(url)?;
    let mut key = canonical.clone();
    if let Some(id) = id {
        key.url = format!("urn:term-squid:valueset-instance:{id}");
    }
    if !scope.cache.contains_key(&key) {
        let request = ValueSetRequest {
            url: &canonical.url,
            version: canonical.version.as_deref(),
            id,
            inline: None,
            system_context: None,
        };
        let expanded = match compose_loader::load(tx, request, &mut scope.budget).await {
            Ok(loaded) => {
                let permit = scope.permit.take().expect("Complex operation work permit");
                let (result, permit) =
                    tokio::task::spawn_blocking(move || (compose_loader::prepare(loaded), permit))
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                scope.permit = Some(permit);
                Some(result?)
            }
            Err(AppError::NativeValueSet) => None,
            Err(e) => return Err(e),
        };
        scope.cache.insert(key.clone(), expanded);
    }
    if let Some(expanded) = &scope.cache[&key] {
        return Ok(expanded.members.find(system, version, code).is_some());
    }
    let result = native_valueset::query_in_transaction(
        tx,
        ValueSetRequest {
            url: &canonical.url,
            version: canonical.version.as_deref(),
            id,
            inline: None,
            system_context: None,
        },
        ValueSetOperation::Find {
            system,
            version,
            code,
        },
    )
    .await?;
    match result {
        ValueSetResult::Native(page) => Ok(!page.members.is_empty()),
        ValueSetResult::Finite(_) => unreachable!("Native query returns a page"),
    }
}
