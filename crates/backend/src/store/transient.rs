//! Immutable request-local definitions. Scoped futures never publish these into retained caches.
use crate::{
    error::AppError,
    models::{ResolvedConcept, ResolvedSubsumption},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use term_squid_core::{Canonical, PreparedConceptMap, PreparedSystem};
use uuid::Uuid;

pub const MAX_RESOURCES: usize = 32;
pub const MAX_BYTES: usize = 1024 * 1024;
pub const MAX_RESOURCE_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    System,
    Set,
    Map,
}

pub struct Entry {
    pub id: Uuid,
    pub resource: Arc<Value>,
    pub bytes: usize,
    pub supplements: Vec<Canonical>,
    pub system: Option<Arc<PreparedSystem>>,
    pub map: Option<Arc<PreparedConceptMap>>,
    ordinal: usize,
}
pub struct Resources {
    entries: BTreeMap<(Kind, Canonical), Arc<Entry>>,
    ids: BTreeMap<Uuid, Arc<Entry>>,
}
tokio::task_local! { static CURRENT: Arc<Resources>; }
pub async fn scope<F: Future>(resources: Arc<Resources>, future: F) -> F::Output {
    CURRENT.scope(resources, future).await
}
pub fn current() -> Option<Arc<Resources>> {
    CURRENT.try_with(Arc::clone).ok()
}
pub fn active() -> bool {
    CURRENT.try_with(|_| ()).is_ok()
}

impl Resources {
    pub fn prepare(
        resources: Vec<Value>,
        date: Option<&str>,
        cancelled: &AtomicBool,
    ) -> Result<Self, AppError> {
        let check = || {
            if cancelled.load(Ordering::Acquire) {
                Err(AppError::Overloaded)
            } else {
                Ok(())
            }
        };
        check()?;
        let cutoff = date.map(super::options::date).transpose()?;
        if resources.is_empty() || resources.len() > MAX_RESOURCES {
            return Err(AppError::BadRequest(
                "tx-resource requires 1..32 resources".into(),
            ));
        }
        let mut raw = BTreeMap::new();
        let mut bytes = 0;
        for (ordinal, resource) in resources.into_iter().enumerate() {
            check()?;
            let kind =
                match resource["resourceType"].as_str() {
                    Some("CodeSystem") => Kind::System,
                    Some("ValueSet") => Kind::Set,
                    Some("ConceptMap") => Kind::Map,
                    _ => return Err(AppError::BadRequest(
                        "tx-resource supports R4 CodeSystem, ValueSet and ConceptMap definitions"
                            .into(),
                    )),
                };
            let url = resource["url"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 4096)
                .ok_or_else(|| {
                    AppError::BadRequest(
                        "tx-resource requires a nonempty canonical url of at most 4096 bytes"
                            .into(),
                    )
                })?;
            if resource
                .get("version")
                .is_some_and(|v| v.as_str().is_none_or(|s| s.is_empty() || s.len() > 4096))
            {
                return Err(AppError::BadRequest(
                    "tx-resource business version must be a nonempty string".into(),
                ));
            }
            if url.contains('|') {
                return Err(AppError::BadRequest(
                    "Resource.url cannot contain a business-version suffix".into(),
                ));
            }
            let key = Canonical::parse(url, resource["version"].as_str())?;
            if kind == Kind::System
                && resource["content"] != "supplement"
                && (url == term_squid_core::ucum::SYSTEM
                    || url == term_squid_core::loinc::SYSTEM
                    || url == term_squid_core::snomed::SYSTEM
                    || url.starts_with("http://snomed.info/sct/"))
            {
                return Err(AppError::BadRequest("tx-resource cannot replace bundled or native authoritative CodeSystem providers".into()));
            }
            let size = serde_json::to_vec(&resource)
                .map_err(|e| AppError::BadRequest(e.to_string()))?
                .len();
            bytes += size;
            if size > MAX_RESOURCE_BYTES || bytes > MAX_BYTES {
                return Err(AppError::BadRequest(
                    "tx-resource exceeds 256 KiB per resource or 1 MiB aggregate".into(),
                ));
            }
            if let Some((previous, previous_ordinal)) = raw.get_mut(&(kind, key.clone())) {
                if previous != &resource {
                    return Err(AppError::BadRequest(
                        "Conflicting tx-resource definitions for one canonical/business version"
                            .into(),
                    ));
                }
                *previous_ordinal = ordinal;
                continue;
            }
            raw.insert((kind, key), (resource, ordinal));
        }
        for ((kind, _), (resource, _)) in &raw {
            if *kind == Kind::System && resource["content"] == "supplement" {
                let target = term_squid_core::supplements::target(resource)?;
                if !raw.iter().any(|((kind, key), (base, _))| {
                    *kind == Kind::System
                        && key.url == target.url
                        && target
                            .version
                            .as_ref()
                            .is_none_or(|v| key.version.as_ref() == Some(v))
                        && base["content"] != "supplement"
                }) {
                    return Err(AppError::BadRequest("Temporary supplements require their finite base CodeSystem in the same request".into()));
                }
            }
        }
        let mut result = Self {
            entries: BTreeMap::new(),
            ids: BTreeMap::new(),
        };
        let mut concepts = 0;
        let mut enriched_bytes = 0;
        for ((kind, key), (resource, ordinal)) in &raw {
            check()?;
            let mut resource = resource.clone();
            let mut supplements = Vec::new();
            let system = if *kind == Kind::System && resource["content"] != "supplement" {
                let mut selected = BTreeMap::new();
                for ((family, supplement_key), (value, ordinal)) in &raw {
                    if *family != Kind::System
                        || value["content"] != "supplement"
                        || value["status"] == "retired"
                    {
                        continue;
                    }
                    let target = term_squid_core::supplements::target(value)?;
                    if target.url != key.url
                        || target
                            .version
                            .as_ref()
                            .is_some_and(|v| Some(v) != key.version.as_ref())
                    {
                        continue;
                    }
                    let published = if let Some(cutoff) = cutoff {
                        let published = value["date"].as_str().ok_or_else(|| {
                            AppError::BadRequest(
                                "Historical temporary supplement requires its publication date"
                                    .into(),
                            )
                        })?;
                        let published = super::options::date(published)?;
                        if published > cutoff {
                            continue;
                        }
                        Some(published)
                    } else {
                        None
                    };
                    let rank = (published, *ordinal);
                    if selected
                        .get(&supplement_key.url)
                        .is_none_or(|(previous, _)| rank > *previous)
                    {
                        selected.insert(supplement_key.url.clone(), (rank, value.clone()));
                    }
                }
                let overlays = selected
                    .into_values()
                    .map(|(_, value)| value)
                    .collect::<Vec<_>>();
                if !overlays.is_empty() {
                    supplements = overlays
                        .iter()
                        .map(|value| {
                            Canonical::parse(
                                value["url"].as_str().expect("Validated supplement URL"),
                                value["version"].as_str(),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    resource = term_squid_core::supplements::apply(&resource, &overlays)?;
                }
                let definition =
                    term_squid_core::CodeSystem::deserialize(&resource).map_err(|e| {
                        AppError::BadRequest(format!("Invalid temporary CodeSystem: {e}"))
                    })?;
                let prepared = PreparedSystem::prepare_for_r4_source(definition)?;
                concepts += prepared.concepts.len();
                if concepts > term_squid_core::MAX_CONCEPTS {
                    return Err(AppError::BadRequest(
                        "tx-resource aggregate concept limit exceeded".into(),
                    ));
                }
                Some(Arc::new(prepared))
            } else {
                None
            };
            if *kind == Kind::System && resource["content"] == "supplement" {
                let target = resource["supplements"].as_str().ok_or_else(|| {
                    AppError::BadRequest(
                        "A temporary supplement requires supplements canonical".into(),
                    )
                })?;
                Canonical::parse(target, None)?;
                concepts += term_squid_core::supplements::codes(&resource)?.len();
                if concepts > term_squid_core::MAX_CONCEPTS {
                    return Err(AppError::BadRequest(
                        "tx-resource aggregate concept limit exceeded".into(),
                    ));
                }
            }
            let map = if *kind == Kind::Map {
                Some(Arc::new(PreparedConceptMap::prepare(&resource)?))
            } else {
                None
            };
            let bytes = serde_json::to_vec(&resource)
                .map_err(|e| AppError::BadRequest(e.to_string()))?
                .len();
            enriched_bytes += bytes;
            if bytes > MAX_BYTES || enriched_bytes > 2 * MAX_BYTES {
                return Err(AppError::BadRequest(
                    "Enriched temporary resource exceeds 1 MiB".into(),
                ));
            }
            let entry = Arc::new(Entry {
                id: Uuid::new_v4(),
                resource: Arc::new(resource),
                bytes,
                supplements,
                system,
                map,
                ordinal: *ordinal,
            });
            result.ids.insert(entry.id, entry.clone());
            result.entries.insert((*kind, key.clone()), entry);
        }
        check()?;
        Ok(result)
    }
    pub fn select(
        &self,
        kind: Kind,
        url: &str,
        version: Option<&str>,
        id: Option<Uuid>,
        date: Option<&str>,
    ) -> Result<Option<Arc<Entry>>, AppError> {
        if let Some(id) = id {
            return Ok(self
                .ids
                .get(&id)
                .filter(|entry| entry.resource["resourceType"] == kind.resource_type())
                .cloned());
        }
        let key = Canonical::parse(url, version)?;
        let mut candidates = self
            .entries
            .iter()
            .filter(|((family, k), _)| {
                *family == kind
                    && k.url == key.url
                    && key
                        .version
                        .as_ref()
                        .is_none_or(|v| k.version.as_ref() == Some(v))
            })
            .map(|(_, entry)| entry.clone())
            .collect::<Vec<_>>();
        if let Some(date) = date {
            let cutoff = super::options::date(date)?;
            let mut eligible = Vec::new();
            for entry in &candidates {
                let published = entry.resource["date"]
                    .as_str()
                    .map(super::options::date)
                    .transpose()?;
                if published.is_some_and(|d| d <= cutoff)
                    || published.is_none() && key.version.is_some()
                {
                    eligible.push((published, entry.ordinal, entry.clone()));
                }
            }
            if !candidates.is_empty() && eligible.is_empty() {
                return Err(AppError::NotFound(
                    "Temporary resource is unavailable for the requested date".into(),
                ));
            }
            eligible.sort_by_key(|(date, ordinal, _)| (*date, *ordinal));
            return Ok(eligible.pop().map(|(_, _, entry)| entry));
        }
        candidates.sort_by_key(|entry| entry.ordinal);
        Ok(candidates.pop())
    }
    pub fn by_id(&self, id: Uuid) -> Option<Arc<Entry>> {
        self.ids.get(&id).cloned()
    }
    pub fn maps(&self, version: Option<&str>) -> Result<Vec<Arc<Entry>>, AppError> {
        let urls = self
            .entries
            .keys()
            .filter(|(kind, _)| *kind == Kind::Map)
            .map(|(_, key)| &key.url)
            .collect::<std::collections::BTreeSet<_>>();
        urls.into_iter()
            .filter_map(|url| self.select(Kind::Map, url, version, None, None).transpose())
            .collect()
    }
}
impl Kind {
    fn resource_type(self) -> &'static str {
        match self {
            Self::System => "CodeSystem",
            Self::Set => "ValueSet",
            Self::Map => "ConceptMap",
        }
    }
}
pub fn select(
    kind: Kind,
    url: &str,
    version: Option<&str>,
    id: Option<Uuid>,
    date: Option<&str>,
) -> Result<Option<Arc<Entry>>, AppError> {
    current()
        .map(|ctx| ctx.select(kind, url, version, id, date))
        .transpose()
        .map(Option::flatten)
}
pub fn by_id(id: Uuid) -> Option<Arc<Entry>> {
    current().and_then(|ctx| ctx.by_id(id))
}
impl Entry {
    pub fn concept(&self, code: &str) -> Result<ResolvedConcept, AppError> {
        let system = self.system.as_ref().ok_or_else(|| {
            AppError::BadRequest("A supplement cannot identify Coding.system".into())
        })?;
        let concept = system.concept(code);
        Ok(ResolvedConcept {
            url: system.canonical.url.clone(),
            native_provider: None,
            name: self.resource["name"].as_str().map(str::to_owned),
            version: system.canonical.version.clone(),
            content_mode: self.resource["content"].as_str().map(str::to_owned),
            code: concept.map(|c| c.code.clone()),
            display: concept.and_then(|c| c.display.clone()),
            definition: concept.and_then(|c| c.definition.clone()),
            properties: concept
                .map(|c| serde_json::to_value(&c.property))
                .transpose()
                .map_err(|e| AppError::Internal(e.into()))?
                .map(sqlx::types::Json),
        })
    }
    pub fn subsumption(&self, a: &str, b: &str) -> Result<ResolvedSubsumption, AppError> {
        let system = self.system.as_ref().ok_or_else(|| {
            AppError::BadRequest("A supplement cannot identify a hierarchy".into())
        })?;
        super::subsumption::require_complete(self.resource["content"] == "complete")?;
        let a = self.concept(a)?.code;
        let b = self.concept(b)?.code;
        let relationship = match (&a, &b) {
            (Some(a), Some(b)) if a != b => {
                if system.ancestors(b)?.contains(a) {
                    Some(true)
                } else if system.ancestors(a)?.contains(b) {
                    Some(false)
                } else {
                    None
                }
            }
            _ => None,
        };
        Ok(ResolvedSubsumption {
            complete: true,
            url: system.canonical.url.clone(),
            version: system.canonical.version.clone(),
            hierarchy_meaning: system.hierarchy_meaning.clone(),
            code_a: a,
            code_b: b,
            relationship,
        })
    }
    pub fn designations(&self, code: &str) -> Result<Vec<Value>, AppError> {
        let system = self.system.as_ref().ok_or_else(|| {
            AppError::BadRequest("A supplement cannot identify Coding.system".into())
        })?;
        let designations = system
            .concept(code)
            .map(|concept| concept.designation.as_slice())
            .unwrap_or_default();
        if designations.len() > 256 {
            return Err(AppError::BadRequest(
                "Concept designations exceed 256 entries".into(),
            ));
        }
        Ok(designations.to_vec())
    }
    pub fn lookup(
        &self,
        request: &super::LookupRequest<'_>,
    ) -> Result<super::LookupAnswer, AppError> {
        super::options::check_resource_date(&self.resource, request.date)?;
        let mut concept = self.concept(request.code)?;
        let designations = self.designations(concept.code.as_deref().unwrap_or(request.code))?;
        let mut properties = Vec::new();
        if request.relationships && self.resource["hierarchyMeaning"] == "is-a" {
            if let (Some(code), Some(system)) = (&concept.code, &self.system) {
                for (kind, code) in system.nested_relationships(code) {
                    if properties.len() >= 10000 {
                        return Err(AppError::BadRequest(
                            "Concept relationship result exceeds 10000 entries".into(),
                        ));
                    }
                    properties.push(serde_json::json!({"code":kind,"valueCode":code}));
                }
            }
        }
        if let Some(language) = request.language {
            super::options::language(language)?;
            if let Some(display) = super::expansion::preferred(&designations, language) {
                concept.display = Some(display.into());
            }
        }
        Ok(super::LookupAnswer {
            concept,
            designations,
            properties,
        })
    }
}
