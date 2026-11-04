//! Optional expansion parameters keep filtering, counts and pages on one snapshot.
use super::{
    compose_loader, native_valueset, selection, ExpansionOptions, ValueSetOperation,
    ValueSetRequest, ValueSetResult,
};
use crate::error::AppError;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use term_squid_core::Canonical;
use tokio::sync::OwnedSemaphorePermit;

pub struct ExpansionPage {
    pub resource: Value,
    pub total: i64,
    pub contains: Vec<Value>,
}
pub(super) async fn run(
    pool: &PgPool,
    mut request: ValueSetRequest<'_>,
    options: &ExpansionOptions,
    filter: Option<&str>,
    offset: i64,
    count: i64,
    permit: OwnedSemaphorePermit,
) -> Result<ExpansionPage, AppError> {
    options.validate()?;
    super::value_set::check_page(offset, count)?;
    let mut tx = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL statement_timeout = '10s'")
        .execute(&mut *tx)
        .await?;
    let context = options
        .context
        .as_deref()
        .map(selection::context)
        .transpose()?;
    if let Some((key, id)) = &context {
        if request.inline.is_some()
            || request.id.is_some()
            || request.url != "urn:term-squid:context"
        {
            return Err(AppError::BadRequest(
                "Choose context, URL, instance or inline ValueSet".into(),
            ));
        }
        request.url = &key.url;
        request.version = key.version.as_deref();
        request.id = *id;
    }
    let mut budget = 32 * 1024 * 1024;
    let implicit = request.inline.is_none() && request.id.is_none() && {
        let canonical = Canonical::parse(request.url, request.version)?;
        term_squid_core::snomed::implicit(&canonical)?.is_some()
            || term_squid_core::loinc::implicit(&canonical)?.is_some()
    };
    let loaded = if implicit {
        None
    } else {
        match compose_loader::load_options(&mut tx, request, &mut budget, Some(options)).await {
            Ok(loaded) => Some(loaded),
            Err(AppError::NativeValueSet) => None,
            Err(e) => return Err(e),
        }
    };
    if let Some(loaded) = loaded {
        tx.commit().await?;
        let options = options.clone();
        let filter = filter.map(str::to_owned);
        return tokio::task::spawn_blocking(move || {
            let _permit = permit;
            finite(loaded, &options, filter.as_deref(), offset, count)
        })
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    let result = native_valueset::query_options(
        &mut tx,
        request,
        ValueSetOperation::Expand {
            filter,
            offset,
            count,
        },
        Some(options),
    )
    .await?;
    let ValueSetResult::Native(page) = result else {
        unreachable!("Native query returns one page")
    };
    let mut contains = Vec::new();
    for member in page.members {
        let mut value = serde_json::to_value(&member).map_err(|e| AppError::Internal(e.into()))?;
        if options.wants_designations() {
            let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT d.language_code,d.type_id,d.term FROM code_systems s JOIN snomed_descriptions d ON d.code_system_id=s.id WHERE s.url=$1 AND s.version IS NOT DISTINCT FROM $2 AND d.concept_code=$3 AND d.active ORDER BY d.id LIMIT 257")
                .bind(&member.system).bind(&member.version).bind(&member.code).fetch_all(&mut *tx).await?;
            if rows.len() > 256 {
                return Err(AppError::BadRequest(
                    "Concept designations exceed 256 entries".into(),
                ));
            }
            let mut designations:Vec<_>=rows.into_iter().map(|(language,kind,term)|json!({"language":language,"use":{"system":"http://snomed.info/sct","code":kind},"value":term})).filter(|d|options.designation_matches(d)).collect();
            let base:uuid::Uuid=sqlx::query_scalar("SELECT id FROM code_systems WHERE url=$1 AND version IS NOT DISTINCT FROM $2 ORDER BY updated_at DESC,id LIMIT 1").bind(&member.system).bind(&member.version).fetch_one(&mut *tx).await?;
            designations.extend(
                super::supplements::designations(
                    &mut tx,
                    base,
                    &member.code,
                    options.date.as_deref(),
                )
                .await?
                .into_iter()
                .filter(|d| options.designation_matches(d)),
            );
            if !designations.is_empty() {
                value["designation"] = designations.into();
            }
        }
        contains.push(value);
    }
    if options.exclude_nested == Some(false) {
        if offset != 0 || page.total > count {
            return Err(AppError::BadRequest(
                "Nested expansion requires a complete page of at most 10000 concepts".into(),
            ));
        }
        let mut groups: BTreeMap<(String, Option<String>), Vec<String>> = BTreeMap::new();
        for value in &contains {
            groups
                .entry((
                    value["system"].as_str().unwrap().to_owned(),
                    value["version"].as_str().map(str::to_owned),
                ))
                .or_default()
                .push(value["code"].as_str().unwrap().to_owned());
        }
        let mut parents = BTreeMap::new();
        let mut edges = 0;
        for ((system, version), codes) in groups {
            let rows:Vec<(String,String)>=sqlx::query_as("SELECT r.source_code,r.destination_code FROM code_systems s JOIN snomed_relationships r ON r.code_system_id=s.id WHERE s.url=$1 AND s.version IS NOT DISTINCT FROM $2 AND r.active AND r.type_id='116680003' AND r.characteristic_type_id='900000000000011006' AND r.source_code=ANY($3::text[]) AND r.destination_code=ANY($3::text[]) ORDER BY 1,2 LIMIT 100001").bind(&system).bind(&version).bind(codes).fetch_all(&mut *tx).await?;
            edges += rows.len();
            if edges > 100000 {
                return Err(AppError::BadRequest(
                    "Nested expansion relationship limit exceeded".into(),
                ));
            }
            for (child, parent) in rows {
                parents
                    .entry((system.clone(), version.clone(), child))
                    .or_insert((system.clone(), version.clone(), parent));
            }
        }
        contains = nest_parents(contains, parents)?;
    }
    tx.commit().await?;
    drop(permit);
    Ok(ExpansionPage {
        resource: page.resource,
        total: page.total,
        contains,
    })
}

type Index<'a> = BTreeMap<(&'a str, Option<&'a str>, &'a str), (&'a Value, Option<&'a str>)>;
fn finite(
    loaded: compose_loader::Loaded,
    options: &ExpansionOptions,
    filter: Option<&str>,
    offset: i64,
    count: i64,
) -> Result<ExpansionPage, AppError> {
    let (expanded, resources) = compose_loader::prepare_rich(loaded)?;
    let mut index = Index::new();
    let mut records = 0;
    for resource in resources.values() {
        let system = resource["url"]
            .as_str()
            .ok_or_else(|| AppError::BadRequest("CodeSystem URL required".into()))?;
        let version = resource["version"].as_str();
        let mut pending: Vec<_> = resource["concept"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| (v, None, 0))
            .collect();
        while let Some((value, parent, depth)) = pending.pop() {
            records += 1;
            if records > 100000 || depth > 32 {
                return Err(AppError::BadRequest(
                    "Expansion metadata traversal limit exceeded".into(),
                ));
            }
            let code = value["code"]
                .as_str()
                .ok_or_else(|| AppError::BadRequest("Concept code required".into()))?;
            index.insert((system, version, code), (value, parent));
            pending.extend(
                value["concept"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|v| (v, Some(code), depth + 1)),
            );
        }
    }
    let excluded = options
        .exclude_system
        .iter()
        .map(|s| Canonical::parse(s, None))
        .collect::<Result<Vec<_>, _>>()?;
    let filter = filter.map(str::to_lowercase);
    let mut total = 0;
    let mut contains = Vec::new();
    for member in expanded.members.values() {
        if options.active_only == Some(true) && member.inactive
            || options.exclude_not_for_ui == Some(true) && member.abstract_code
            || excluded.iter().any(|s| {
                s.url == member.system
                    && s.version
                        .as_ref()
                        .is_none_or(|v| Some(v) == member.version.as_ref())
            })
        {
            continue;
        }
        let metadata = index.get(&(
            member.system.as_str(),
            member.version.as_deref(),
            member.code.as_str(),
        ));
        let concept = metadata.map(|(v, _)| *v);
        if options.exclude_post_coordinated == Some(true)
            && (member.system == term_squid_core::snomed::SYSTEM
                && member.code.chars().any(|c| matches!(c, ':' | '+'))
                || concept.is_some_and(|c| {
                    c["property"].as_array().is_some_and(|p| {
                        p.iter()
                            .any(|p| p["code"] == "postCoordinated" && p["valueBoolean"] == true)
                    })
                }))
        {
            continue;
        }
        let designations = concept.and_then(|c| c["designation"].as_array());
        let localized = options
            .display_language
            .as_deref()
            .and_then(|language| designations.and_then(|ds| preferred(ds, language)));
        let display = localized.or(member.display.as_deref());
        if filter.as_ref().is_some_and(|f| {
            !member.code.to_lowercase().contains(f)
                && display.is_none_or(|d| !d.to_lowercase().contains(f))
        }) {
            continue;
        }
        if total >= offset && contains.len() < count as usize {
            let mut value =
                serde_json::to_value(member).map_err(|e| AppError::Internal(e.into()))?;
            if let Some(display) = display {
                value["display"] = display.into();
            }
            if options.wants_designations() {
                if let Some(designations) = designations {
                    if designations.len() > 256 {
                        return Err(AppError::BadRequest(
                            "Concept designations exceed 256 entries".into(),
                        ));
                    }
                    let selected: Vec<_> = designations
                        .iter()
                        .filter(|d| options.designation_matches(d))
                        .cloned()
                        .collect();
                    if !selected.is_empty() {
                        value["designation"] = selected.into();
                    }
                }
            }
            contains.push(value);
        }
        total += 1;
    }
    if options.exclude_nested == Some(false) {
        if offset != 0 || total > count {
            return Err(AppError::BadRequest(
                "Nested expansion requires a complete page of at most 10000 concepts".into(),
            ));
        }
        contains = nest(contains, &index)?;
    }
    Ok(ExpansionPage {
        resource: expanded.resource.as_ref().clone(),
        total,
        contains,
    })
}
pub(super) fn display_language_range<'a>(
    designations: &[Value],
    language: &'a str,
) -> Option<&'a str> {
    let mut range = language;
    loop {
        if designations.iter().any(|d| {
            d["language"]
                .as_str()
                .is_some_and(|actual| super::options::language_matches(range, actual))
        }) {
            return Some(range);
        }
        range = range.rsplit_once('-')?.0;
    }
}
pub(super) fn preferred<'a>(designations: &'a [Value], language: &str) -> Option<&'a str> {
    let mut range = language;
    loop {
        if let Some(value) = designations
            .iter()
            .filter(|d| {
                d["language"]
                    .as_str()
                    .is_some_and(|actual| super::options::language_matches(range, actual))
            })
            .min_by_key(|d| {
                (
                    d["language"]
                        .as_str()
                        .is_none_or(|l| !l.eq_ignore_ascii_case(range)),
                    d["use"]["code"] != "display",
                )
            })
            .and_then(|d| d["value"].as_str())
        {
            return Some(value);
        }
        range = range.rsplit_once('-')?.0;
    }
}
type CodeKey = (String, Option<String>, String);
type Children = BTreeMap<CodeKey, Vec<CodeKey>>;
fn nest(values: Vec<Value>, index: &Index<'_>) -> Result<Vec<Value>, AppError> {
    let key = |value: &Value| {
        (
            value["system"].as_str().unwrap_or("").to_owned(),
            value["version"].as_str().map(str::to_owned),
            value["code"].as_str().unwrap_or("").to_owned(),
        )
    };
    let known: BTreeSet<_> = values.iter().map(&key).collect();
    let parents = known
        .iter()
        .filter_map(|code| {
            index
                .get(&(code.0.as_str(), code.1.as_deref(), code.2.as_str()))
                .and_then(|(_, p)| *p)
                .map(|p| (code.clone(), (code.0.clone(), code.1.clone(), p.to_owned())))
        })
        .collect();
    nest_parents(values, parents)
}
fn nest_parents(
    values: Vec<Value>,
    parents: BTreeMap<CodeKey, CodeKey>,
) -> Result<Vec<Value>, AppError> {
    let mut nodes: BTreeMap<CodeKey, _> = values
        .into_iter()
        .map(|v| {
            (
                (
                    v["system"].as_str().unwrap_or("").to_owned(),
                    v["version"].as_str().map(str::to_owned),
                    v["code"].as_str().unwrap_or("").to_owned(),
                ),
                v,
            )
        })
        .collect();
    let mut children: BTreeMap<_, Vec<_>> = BTreeMap::new();
    let mut roots = BTreeSet::new();
    for code in nodes.keys() {
        let parent = parents
            .get(code)
            .filter(|p| nodes.contains_key(*p))
            .cloned();
        if let Some(parent) = parent {
            children.entry(parent).or_default().push(code.clone());
        } else {
            roots.insert(code.clone());
        }
    }
    fn take(
        key: CodeKey,
        nodes: &mut BTreeMap<CodeKey, Value>,
        children: &Children,
        depth: usize,
    ) -> Result<Value, AppError> {
        if depth > 32 {
            return Err(AppError::BadRequest(
                "Nested expansion depth exceeds 32".into(),
            ));
        }
        let mut value = nodes
            .remove(&key)
            .ok_or_else(|| AppError::BadRequest("Cyclic expansion hierarchy".into()))?;
        if let Some(children_list) = children.get(&key) {
            value["contains"] = children_list
                .iter()
                .map(|c| take(c.clone(), nodes, children, depth + 1))
                .collect::<Result<Vec<_>, _>>()?
                .into();
        }
        Ok(value)
    }
    let result = roots
        .into_iter()
        .map(|r| take(r, &mut nodes, &children, 0))
        .collect::<Result<Vec<_>, _>>()?;
    if !nodes.is_empty() {
        return Err(AppError::BadRequest("Cyclic expansion hierarchy".into()));
    }
    Ok(result)
}
