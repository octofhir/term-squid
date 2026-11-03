//! Storage-independent terminology primitives and bounded, immutable compose evaluation.
mod concepts;
pub mod loinc;
pub mod snomed;
pub mod ucum;
pub use concepts::{PreparedConcepts, ProjectedConcept};
mod translation;
pub use translation::{PreparedConceptMap, TranslationMatch};
mod hierarchy;
pub use hierarchy::{
    HierarchyConcept, HierarchyMatch, PreparedHierarchy, MAX_HIERARCHY_RELATIONSHIPS,
};

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DEPTH: usize = 32;
pub const MAX_CONCEPTS: usize = 100_000;
pub const MAX_RESOURCES: usize = 128;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);
type Result<T> = std::result::Result<T, Error>;

/// A business version is independent of the FHIR meta.versionId.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Canonical {
    pub url: String,
    pub version: Option<String>,
}
impl Canonical {
    pub fn parse(url: &str, version: Option<&str>) -> Result<Self> {
        let (url, embedded) = url
            .split_once('|')
            .map_or((url, None), |(u, v)| (u, Some(v)));
        if url.is_empty() || embedded.is_some_and(|v| v.is_empty() || v.contains('|')) {
            return Err(Error("Invalid canonical".into()));
        }
        if embedded.zip(version).is_some_and(|(a, b)| a != b) {
            return Err(Error("Conflicting canonical and parameter versions".into()));
        }
        Ok(Self {
            url: url.into(),
            version: embedded.or(version).map(str::to_owned),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Property {
    pub code: String,
    #[serde(flatten)]
    pub value: BTreeMap<String, serde_json::Value>,
}
impl Property {
    pub fn scalar(&self) -> Option<&serde_json::Value> {
        self.value
            .iter()
            .find(|(k, _)| k.starts_with("value"))
            .map(|(_, v)| v)
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Concept {
    pub code: String,
    pub display: Option<String>,
    pub definition: Option<String>,
    #[serde(default)]
    pub property: Vec<Property>,
    #[serde(default)]
    pub concept: Vec<Concept>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyDefinition {
    pub code: String,
    pub uri: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeSystem {
    pub url: String,
    pub version: Option<String>,
    pub content: String,
    pub case_sensitive: Option<bool>,
    pub hierarchy_meaning: Option<String>,
    #[serde(default)]
    pub property: Vec<PropertyDefinition>,
    #[serde(default)]
    pub concept: Vec<Concept>,
}
#[derive(Debug)]
pub struct PreparedSystem {
    pub canonical: Canonical,
    pub case_sensitive: bool,
    pub hierarchy_meaning: Option<String>,
    pub properties: BTreeSet<String>,
    pub concepts: BTreeMap<String, Concept>,
    pub parents: BTreeMap<String, BTreeSet<String>>,
}
impl PreparedSystem {
    pub fn prepare(system: CodeSystem) -> Result<Self> {
        Self::prepare_inner(system, true)
    }
    pub fn prepare_for_import(system: CodeSystem) -> Result<Self> {
        Self::prepare_inner(system, false)
    }
    fn prepare_inner(system: CodeSystem, require_complete: bool) -> Result<Self> {
        if require_complete && system.content != "complete" {
            return Err(Error(format!(
                "CodeSystem {} has unsupported content mode {}",
                system.url, system.content
            )));
        }
        // Property URIs carry meaning; do not silently treat an alias as unrelated data.
        for property in &system.property {
            let graph = matches!(property.code.as_str(), "parent" | "child");
            let flags = matches!(
                property.code.as_str(),
                "inactive" | "notSelectable" | "abstract"
            );
            if (graph || (require_complete && flags))
                && property.uri.as_deref().is_some_and(|uri| {
                    uri.strip_prefix("http://hl7.org/fhir/concept-properties#")
                        != Some(property.code.as_str())
                })
            {
                return Err(Error(format!(
                    "Custom meaning for property {} requires a provider",
                    property.code
                )));
            }
            if let Some(kind) = property
                .uri
                .as_deref()
                .and_then(|u| u.strip_prefix("http://hl7.org/fhir/concept-properties#"))
            {
                if (matches!(kind, "parent" | "child")
                    || (require_complete
                        && matches!(
                            kind,
                            "inactive"
                                | "notSelectable"
                                | "status"
                                | "retirementDate"
                                | "deprecationDate"
                        )))
                    && property.code != kind
                {
                    return Err(Error(
                        "Standard property URI aliases require a provider".into(),
                    ));
                }
            }
        }
        let complete = system.content == "complete";
        let mut prepared = Self {
            canonical: Canonical::parse(&system.url, system.version.as_deref())?,
            case_sensitive: system.case_sensitive.unwrap_or(true),
            hierarchy_meaning: system.hierarchy_meaning,
            properties: system.property.into_iter().map(|p| p.code).collect(),
            concepts: BTreeMap::new(),
            parents: BTreeMap::new(),
        };
        let mut pending: Vec<_> = system.concept.into_iter().map(|c| (c, None, 0)).collect();
        while let Some((mut concept, parent, depth)) = pending.pop() {
            if depth > MAX_DEPTH || prepared.concepts.len() >= MAX_CONCEPTS {
                return Err(Error("CodeSystem size or nesting limit exceeded".into()));
            }
            if require_complete
                && concept.property.iter().any(|p| {
                    matches!(
                        p.code.as_str(),
                        "status" | "retirementDate" | "deprecationDate"
                    )
                })
            {
                return Err(Error("Lifecycle status properties require provider-specific rules; composition is not supported".into()));
            }
            let code = concept.code.clone();
            let parents = prepared.parents.entry(code.clone()).or_default();
            if let Some(parent) = parent {
                parents.insert(parent);
            }
            for p in &concept.property {
                if p.code == "parent" {
                    let parent = p
                        .scalar()
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| Error("Invalid parent property".into()))?;
                    parents.insert(parent.into());
                }
            }
            for child in std::mem::take(&mut concept.concept) {
                pending.push((child, Some(code.clone()), depth + 1));
            }
            if prepared.concepts.insert(code, concept).is_some() {
                return Err(Error("Duplicate concept code".into()));
            }
        }
        for (child, c) in &prepared.concepts {
            for p in &c.property {
                if p.code == "child" {
                    let descendant = p
                        .scalar()
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| Error("Invalid child property".into()))?;
                    prepared
                        .parents
                        .entry(descendant.into())
                        .or_default()
                        .insert(child.clone());
                }
            }
        }
        if !prepared.case_sensitive {
            let folded: BTreeSet<_> = prepared.concepts.keys().map(|s| s.to_lowercase()).collect();
            if folded.len() != prepared.concepts.len() {
                return Err(Error("Ambiguous case-insensitive codes".into()));
            }
        }
        if !complete {
            for parents in prepared.parents.values_mut() {
                parents.retain(|p| prepared.concepts.contains_key(p));
            }
        }
        // Validate cycles and dangling references once during preparation.
        for code in prepared.concepts.keys() {
            prepared.ancestors(code)?;
        }
        Ok(prepared)
    }
    pub fn ancestor_depths(&self, code: &str) -> Result<BTreeMap<String, usize>> {
        let mut depths = BTreeMap::new();
        let mut pending = std::collections::VecDeque::from([(code.to_owned(), 0)]);
        while let Some((c, depth)) = pending.pop_front() {
            if depth > MAX_DEPTH {
                return Err(Error("Hierarchy depth limit exceeded".into()));
            }
            for parent in self.parents.get(&c).into_iter().flatten() {
                if parent == code {
                    return Err(Error("Hierarchy cycle".into()));
                }
                if !self.concepts.contains_key(parent) {
                    return Err(Error(format!("Unknown parent {parent}")));
                }
                if !depths.contains_key(parent) {
                    depths.insert(parent.clone(), depth + 1);
                    pending.push_back((parent.clone(), depth + 1));
                }
            }
        }
        Ok(depths)
    }
    pub fn ancestors(&self, code: &str) -> Result<BTreeSet<String>> {
        Ok(self.ancestor_depths(code)?.into_keys().collect())
    }
}
#[derive(Debug, Clone, Deserialize)]
pub struct Filter {
    pub property: String,
    pub op: String,
    pub value: String,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Selection {
    pub system: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub concept: Vec<ExplicitConcept>,
    #[serde(default)]
    pub filter: Vec<Filter>,
    #[serde(default, rename = "valueSet")]
    pub value_set: Vec<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct ExplicitConcept {
    pub code: String,
    pub display: Option<String>,
}
#[derive(Debug, Clone, Deserialize)]
pub struct Compose {
    pub inactive: Option<bool>,
    #[serde(rename = "lockedDate")]
    pub locked_date: Option<String>,
    pub include: Vec<Selection>,
    #[serde(default)]
    pub exclude: Vec<Selection>,
}
impl Compose {
    pub fn dependencies(&self) -> Result<(Vec<Canonical>, Vec<Canonical>)> {
        let dependencies = self.references()?;
        for s in self.include.iter().chain(&self.exclude) {
            for f in &s.filter {
                if !matches!(
                    (f.property.as_str(), f.op.as_str()),
                    ("concept", "is-a" | "descendent-of" | "is-not-a") | (_, "=" | "exists")
                ) {
                    return Err(Error(format!("Unsupported filter {} {}", f.property, f.op)));
                }
                if f.op == "exists" && f.value != "true" && f.value != "false" {
                    return Err(Error("exists expects true or false".into()));
                }
            }
        }
        Ok(dependencies)
    }
    /// Structural checks and references; providers validate their own filter language.
    pub fn references(&self) -> Result<(Vec<Canonical>, Vec<Canonical>)> {
        if self.include.is_empty() {
            return Err(Error("compose.include must not be empty".into()));
        }
        if self.locked_date.is_some() {
            return Err(Error("lockedDate is not supported".into()));
        }
        let mut systems = Vec::new();
        let mut sets = Vec::new();
        for s in self.include.iter().chain(&self.exclude) {
            if s.system.is_none()
                && (!s.concept.is_empty() || !s.filter.is_empty() || s.version.is_some())
            {
                return Err(Error(
                    "Concepts, filters and version require a system".into(),
                ));
            }
            if !s.concept.is_empty() && !s.filter.is_empty() {
                return Err(Error("concept and filter cannot coexist".into()));
            }
            if s.system.is_none() && s.value_set.is_empty() {
                return Err(Error("Empty compose selection".into()));
            }
            if let Some(system) = &s.system {
                systems.push(Canonical::parse(system, s.version.as_deref())?);
            }
            for set in &s.value_set {
                sets.push(Canonical::parse(set, None)?);
            }
        }
        Ok((systems, sets))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MemberKey {
    pub system: String,
    pub version: Option<String>,
    pub code: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub system: String,
    #[serde(skip)]
    pub case_sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub inactive: bool,
    #[serde(rename = "abstract", skip_serializing_if = "std::ops::Not::not")]
    pub abstract_code: bool,
}
pub type Members = BTreeMap<MemberKey, Member>;

/// Immutable dependency snapshot constructed in one repeatable-read transaction.
#[derive(Default)]
pub struct Snapshot {
    pub systems: BTreeMap<Canonical, PreparedSystem>,
    pub value_sets: BTreeMap<Canonical, Compose>,
}
impl Snapshot {
    pub fn expand(&self, canonical: &Canonical) -> Result<Members> {
        self.evaluate(canonical, &mut BTreeSet::new())
    }
    fn evaluate(&self, canonical: &Canonical, path: &mut BTreeSet<Canonical>) -> Result<Members> {
        if path.len() >= MAX_DEPTH || !path.insert(canonical.clone()) {
            return Err(Error("ValueSet import cycle or depth limit".into()));
        }
        let compose = self
            .value_sets
            .get(canonical)
            .ok_or_else(|| Error(format!("Unresolved ValueSet {}", canonical.url)))?;
        compose.dependencies()?;
        let mut result = Members::new();
        for selection in &compose.include {
            result.extend(self.select(selection, path)?);
            if result.len() > MAX_CONCEPTS {
                return Err(Error("Expansion size limit exceeded".into()));
            }
        }
        for selection in &compose.exclude {
            let excluded = self.select(selection, path)?;
            if let Some(system) = &selection.system {
                let canonical = Canonical::parse(system, selection.version.as_deref())?;
                if canonical.version.is_none()
                    && selection.concept.is_empty()
                    && selection.filter.is_empty()
                    && selection.value_set.is_empty()
                {
                    result.retain(|k, _| k.system != canonical.url);
                    continue;
                }
            }
            let codes: BTreeSet<_> = excluded.keys().map(|k| (&k.system, &k.code)).collect();
            let versioned = selection
                .system
                .as_ref()
                .map(|s| Canonical::parse(s, selection.version.as_deref()))
                .transpose()?
                .is_some_and(|c| c.version.is_some());
            result.retain(|k, _| {
                if versioned {
                    !excluded.contains_key(k)
                } else {
                    !codes.contains(&(&k.system, &k.code))
                }
            });
        }
        if compose.inactive == Some(false) {
            result.retain(|_, v| !v.inactive);
        }
        path.remove(canonical);
        Ok(result)
    }
    fn select(&self, selection: &Selection, path: &mut BTreeSet<Canonical>) -> Result<Members> {
        let mut result: Option<Members> = None;
        if let Some(system) = &selection.system {
            let canonical = Canonical::parse(system, selection.version.as_deref())?;
            let system = self
                .systems
                .get(&canonical)
                .ok_or_else(|| Error(format!("Unresolved CodeSystem {}", canonical.url)))?;
            for f in &selection.filter {
                if f.property == "concept" && f.op != "=" && f.op != "exists" {
                    if system.hierarchy_meaning.as_deref() != Some("is-a") {
                        return Err(Error("Hierarchy is not declared as is-a".into()));
                    }
                    if !system.concepts.contains_key(&f.value) {
                        return Err(Error(format!("Unknown hierarchy code {}", f.value)));
                    }
                } else if f.property != "code"
                    && f.property != "concept"
                    && !system.properties.contains(&f.property)
                    && !system
                        .concepts
                        .values()
                        .any(|c| c.property.iter().any(|p| p.code == f.property))
                {
                    return Err(Error(format!("Unknown filter property {}", f.property)));
                }
            }
            let normalize = |code: &str| {
                if system.case_sensitive {
                    code.to_owned()
                } else {
                    code.to_lowercase()
                }
            };
            let explicit: BTreeMap<_, _> = selection
                .concept
                .iter()
                .map(|c| (normalize(&c.code), c.display.as_ref()))
                .collect();
            let known: BTreeSet<_> = system.concepts.keys().map(|c| normalize(c)).collect();
            for code in explicit.keys() {
                if !known.contains(code) {
                    return Err(Error(format!("Unknown explicit code {code}")));
                }
            }
            let mut members = Members::new();
            for (code, concept) in &system.concepts {
                if !explicit.is_empty() && !explicit.contains_key(&normalize(code)) {
                    continue;
                }
                let mut matches = true;
                for filter in &selection.filter {
                    matches &= filter_matches(system, concept, filter)?;
                }
                if !matches {
                    continue;
                }
                let inactive = bool_property(concept, "inactive");
                let member = Member {
                    system: system.canonical.url.clone(),
                    case_sensitive: system.case_sensitive,
                    version: system.canonical.version.clone(),
                    code: code.clone(),
                    display: explicit
                        .get(&normalize(code))
                        .and_then(|d| (*d).cloned())
                        .or_else(|| concept.display.clone()),
                    inactive,
                    abstract_code: bool_property(concept, "notSelectable")
                        || bool_property(concept, "abstract"),
                };
                members.insert(
                    MemberKey {
                        system: member.system.clone(),
                        version: member.version.clone(),
                        code: code.clone(),
                    },
                    member,
                );
            }
            result = Some(members);
        }
        // Multiple valueSet references in one selection are an intersection (FHIR R4).
        for imported in &selection.value_set {
            let members = self.evaluate(&Canonical::parse(imported, None)?, path)?;
            if let Some(existing) = &mut result {
                existing.retain(|key, _| members.contains_key(key));
            } else {
                result = Some(members);
            }
        }
        Ok(result.unwrap_or_default())
    }
}
fn bool_property(c: &Concept, name: &str) -> bool {
    c.property
        .iter()
        .any(|p| p.code == name && p.scalar().and_then(|v| v.as_bool()) == Some(true))
}
fn filter_matches(s: &PreparedSystem, c: &Concept, f: &Filter) -> Result<bool> {
    if f.property == "concept" && matches!(f.op.as_str(), "is-a" | "descendent-of" | "is-not-a") {
        let descendant = s.ancestors(&c.code)?.contains(&f.value);
        return Ok(match f.op.as_str() {
            "is-a" => c.code == f.value || descendant,
            "descendent-of" => descendant,
            _ => c.code != f.value && !descendant,
        });
    }
    let values: Vec<_> = c.property.iter().filter(|p| p.code == f.property).collect();
    Ok(match f.op.as_str() {
        "=" if f.property == "concept" || f.property == "code" => {
            if s.case_sensitive {
                c.code == f.value
            } else {
                c.code.to_lowercase() == f.value.to_lowercase()
            }
        }
        "=" => {
            let mut matched = false;
            for property in values {
                matched |= equality_matches(property, &f.value)?;
            }
            matched
        }
        "exists" if f.property == "concept" || f.property == "code" => f.value == "true",
        "exists" => (!values.is_empty()) == (f.value == "true"),
        _ => return Err(Error("Unsupported filter".into())),
    })
}

// Do not stringify Coding/decimal/date-time values into an invented comparison language.
fn equality_matches(property: &Property, expected: &str) -> Result<bool> {
    let values: Vec<_> = property
        .value
        .iter()
        .filter(|(key, _)| key.starts_with("value"))
        .collect();
    if values.len() != 1 {
        return Err(Error("A property must have exactly one typed value".into()));
    }
    let (kind, value) = values[0];
    Ok(match kind.as_str() {
        "valueCode" | "valueString" => {
            value
                .as_str()
                .ok_or_else(|| Error("Invalid string property".into()))?
                == expected
        }
        "valueBoolean" => {
            value
                .as_bool()
                .ok_or_else(|| Error("Invalid boolean property".into()))?
                == expected
                    .parse::<bool>()
                    .map_err(|_| Error("Boolean filters require true or false".into()))?
        }
        "valueInteger" => {
            let actual = value
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| Error("Invalid integer property".into()))?;
            actual
                == expected
                    .parse::<i32>()
                    .map_err(|_| Error("Integer filters require a 32-bit integer".into()))?
        }
        _ => {
            return Err(Error(format!(
                "Equality for property type {kind} is not supported"
            )))
        }
    })
}

#[cfg(test)]
mod tests;

/// Immutable finite ValueSet plan. Evaluation and normalization happen once, never per code.
/// The index preserves deterministic version ordering and case-insensitive system semantics.
pub struct PreparedValueSet {
    members: Members,
    index: BTreeMap<(String, String), Vec<MemberKey>>,
}
impl PreparedValueSet {
    pub fn prepare(snapshot: &Snapshot, canonical: &Canonical) -> Result<Self> {
        let members = snapshot.expand(canonical)?;
        let mut index: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (key, member) in &members {
            let code = if member.case_sensitive {
                member.code.clone()
            } else {
                member.code.to_lowercase()
            };
            index
                .entry((member.system.clone(), code))
                .or_default()
                .push(key.clone());
        }
        Ok(Self { members, index })
    }
    pub fn values(&self) -> impl Iterator<Item = &Member> {
        self.members.values()
    }
    pub fn find(&self, system: &str, version: Option<&str>, code: &str) -> Option<&Member> {
        // A URL may have both case-sensitive and insensitive versions.
        let exact = self.index.get(&(system.to_owned(), code.to_owned()));
        let folded = self.index.get(&(system.to_owned(), code.to_lowercase()));
        exact
            .into_iter()
            .chain(folded)
            .flatten()
            .filter_map(|k| self.members.get(k))
            .filter(|m| {
                (m.code == code
                    || (!m.case_sensitive && m.code.to_lowercase() == code.to_lowercase()))
                    && version.is_none_or(|v| m.version.as_deref() == Some(v))
            })
            .min_by(|a, b| a.version.cmp(&b.version).then(a.code.cmp(&b.code)))
    }
    /// Conservative charge for retained string buffers, tree nodes and duplicate index keys.
    pub fn memory_charge(&self) -> usize {
        self.members
            .values()
            .map(|m| {
                2048 + 16
                    * (m.system.len()
                        + m.version.as_ref().map_or(0, String::len)
                        + m.code.len()
                        + m.display.as_ref().map_or(0, String::len))
            })
            .sum()
    }
}
