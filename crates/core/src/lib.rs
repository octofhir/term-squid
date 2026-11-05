//! Storage-independent terminology primitives and bounded, immutable compose evaluation.
mod code_index;
mod concepts;
pub use code_index::CodeSystemIndex;
mod filters;
mod graph;
mod properties;
pub use properties::PropertyDefinitions;
pub mod loinc;
pub mod snomed;
pub mod supplements;
pub mod ucum;
pub use concepts::{PreparedConcepts, ProjectedConcept};
mod translation;
pub use translation::{
    PreparedConceptMap, ResolvedTranslation, TranslationContinuation, TranslationDependency,
    TranslationMatch, TranslationProduct, TranslationResolution, VersionedTranslationMatch,
};
mod hierarchy;
pub use hierarchy::{
    HierarchyConcept, HierarchyMatch, PreparedHierarchy, MAX_HIERARCHY_RELATIONSHIPS,
};

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

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
    pub fn typed_value(&self) -> Result<(&str, &serde_json::Value)> {
        let mut values = self
            .value
            .iter()
            .filter(|(key, _)| key.starts_with("value"));
        let value = values
            .next()
            .ok_or_else(|| Error("A property must have exactly one typed value".into()))?;
        if values.next().is_some() {
            return Err(Error("A property must have exactly one typed value".into()));
        }
        Ok((value.0.as_str(), value.1))
    }
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
    pub designation: Vec<serde_json::Value>,
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
    #[serde(rename = "type")]
    pub kind: Option<String>,
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
    r4_semantics: bool,
    explicit_projection: bool,
    // Lookup can retain opaque lifecycle metadata; composition must preserve its error.
    composition_error: Option<String>,
    content_mode: String,
    definitions: PropertyDefinitions,
    folded: BTreeMap<String, String>,
    nested: BTreeMap<String, (Option<String>, Vec<String>)>,
    pub canonical: Canonical,
    pub case_sensitive: bool,
    pub hierarchy_meaning: Option<String>,
    pub properties: BTreeSet<String>,
    pub concepts: BTreeMap<String, Concept>,
    pub parents: BTreeMap<String, BTreeSet<String>>,
}
impl PreparedSystem {
    pub fn prepare(system: CodeSystem) -> Result<Self> {
        Self::prepare_inner(system, true, false)
    }
    /// Enable bounded R4 scalar filters and formal boolean/status property semantics.
    /// Undeclared lifecycle policies, graph aliases and date transitions remain explicit errors.
    pub fn prepare_for_r4(system: CodeSystem) -> Result<Self> {
        Self::prepare_inner(system, true, true)
    }
    /// Validate R4 property values while preserving partial-source lookup/validation.
    /// Incomplete sources still cannot establish an unknown code as conclusively invalid.
    pub fn prepare_for_r4_source(system: CodeSystem) -> Result<Self> {
        Self::prepare_inner(system, false, true)
    }
    pub fn prepare_for_import(system: CodeSystem) -> Result<Self> {
        Self::prepare_inner(system, false, false)
    }
    fn prepare_inner(
        system: CodeSystem,
        require_complete: bool,
        r4_semantics: bool,
    ) -> Result<Self> {
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
            if (graph || (require_complete && flags && !r4_semantics))
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
                        && !r4_semantics
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
        let declared_status = r4_semantics
            && system.property.iter().any(|property| {
                property.uri.as_deref() == Some("http://hl7.org/fhir/concept-properties#status")
            });
        let complete = system.content == "complete";
        let mut prepared = Self {
            r4_semantics,
            explicit_projection: false,
            composition_error: None,
            content_mode: system.content,
            definitions: PropertyDefinitions::prepare(&system.property)?,
            folded: BTreeMap::new(),
            nested: BTreeMap::new(),
            canonical: Canonical::parse(&system.url, system.version.as_deref())?,
            case_sensitive: system.case_sensitive.unwrap_or(true),
            hierarchy_meaning: system.hierarchy_meaning,
            properties: system.property.into_iter().map(|p| p.code).collect(),
            concepts: BTreeMap::new(),
            parents: BTreeMap::new(),
        };
        if r4_semantics {
            prepared.definitions.validate_r4_types()?;
        }
        let mut pending: Vec<_> = system.concept.into_iter().map(|c| (c, None, 0)).collect();
        while let Some((mut concept, parent, depth)) = pending.pop() {
            if depth > MAX_DEPTH || prepared.concepts.len() >= MAX_CONCEPTS {
                return Err(Error("CodeSystem size or nesting limit exceeded".into()));
            }
            if r4_semantics && concept.code.is_empty() {
                return Err(Error("R4 concept code must not be empty".into()));
            }
            if r4_semantics {
                let mut flags = [None; 3];
                for property in &concept.property {
                    prepared.definitions.validate_value(property)?;
                    let index = match prepared.property_meaning(&property.code) {
                        Some("inactive") => Some(0),
                        Some("notSelectable") => Some(1),
                        Some("abstract") => Some(2),
                        _ => None,
                    };
                    if let Some((index, value)) = index.zip(
                        property
                            .value
                            .get("valueBoolean")
                            .and_then(serde_json::Value::as_bool),
                    ) {
                        if flags[index]
                            .replace(value)
                            .is_some_and(|previous| previous != value)
                        {
                            return Err(Error(
                                "Contradictory standard boolean concept properties".into(),
                            ));
                        }
                    }
                }
            }
            if require_complete || r4_semantics && prepared.composition_error.is_none() {
                let lifecycle = prepared.validate_lifecycle(&concept, declared_status);
                if require_complete {
                    lifecycle?;
                } else if let Err(Error(error)) = lifecycle {
                    prepared.composition_error.get_or_insert(error);
                }
            }
            if r4_semantics
                && concept.property.iter().any(|property| {
                    matches!(
                        prepared.property_meaning(&property.code),
                        Some("inactive" | "notSelectable" | "abstract")
                    ) && (property.typed_value().is_err()
                        || property
                            .value
                            .get("valueBoolean")
                            .and_then(serde_json::Value::as_bool)
                            .is_none())
                })
            {
                return Err(Error(
                    "Standard boolean concept properties require one valueBoolean".into(),
                ));
            }
            let code = concept.code.clone();
            prepared.nested.insert(
                code.clone(),
                (
                    parent.clone(),
                    concept
                        .concept
                        .iter()
                        .map(|child| child.code.clone())
                        .collect(),
                ),
            );
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
            for code in prepared.concepts.keys() {
                if prepared
                    .folded
                    .insert(code.to_lowercase(), code.clone())
                    .is_some()
                {
                    return Err(Error("Ambiguous case-insensitive codes".into()));
                }
            }
        }
        if !complete {
            for parents in prepared.parents.values_mut() {
                parents.retain(|p| prepared.concepts.contains_key(p));
            }
        }
        prepared.validate_graph(complete)?;
        Ok(prepared)
    }
    fn validate_lifecycle(&self, concept: &Concept, declared_status: bool) -> Result<()> {
        if concept.property.iter().any(|p| {
            matches!(
                self.property_meaning(&p.code),
                Some("retirementDate" | "deprecationDate")
            ) || (self.property_meaning(&p.code) == Some("status") || p.code == "status")
                && !declared_status
        }) {
            return Err(Error("Lifecycle status properties require provider-specific rules; composition is not supported".into()));
        }
        if declared_status {
            let statuses = concept
                .property
                .iter()
                .filter(|p| self.property_meaning(&p.code) == Some("status"))
                .collect::<Vec<_>>();
            if statuses.len() > 1
                || statuses.iter().any(|property| {
                    property.typed_value().is_err()
                        || !matches!(
                            property
                                .value
                                .get("valueCode")
                                .and_then(serde_json::Value::as_str),
                            Some("active" | "experimental" | "deprecated" | "retired")
                        )
                })
            {
                return Err(Error(
                    "Lifecycle status requires one standard R4 valueCode".into(),
                ));
            }
            if statuses
                .first()
                .is_some_and(|property| property.value["valueCode"] == "retired")
                && concept.property.iter().any(|property| {
                    self.property_meaning(&property.code) == Some("inactive")
                        && property.value.get("valueBoolean")
                            == Some(&serde_json::Value::Bool(false))
                })
            {
                return Err(Error(
                    "Lifecycle status retired conflicts with inactive=false".into(),
                ));
            }
        }
        Ok(())
    }
    /// Reuse an unchanged R4 preparation while retaining strict compose admission.
    pub fn require_r4_composition(&self) -> Result<()> {
        if !self.r4_semantics {
            return Err(Error(
                "Shared composition requires an R4 prepared source".into(),
            ));
        }
        if self.content_mode != "complete" && !self.explicit_projection {
            return Err(Error(format!(
                "CodeSystem {} has unsupported content mode {}",
                self.canonical.url, self.content_mode
            )));
        }
        if let Some(error) = &self.composition_error {
            return Err(Error(error.clone()));
        }
        Ok(())
    }
    pub fn canonical_code(&self, code: &str) -> Option<&str> {
        if self.case_sensitive {
            self.concepts
                .get_key_value(code)
                .map(|(code, _)| code.as_str())
        } else {
            self.folded.get(&code.to_lowercase()).map(String::as_str)
        }
    }
    pub fn concept(&self, code: &str) -> Option<&Concept> {
        self.canonical_code(code)
            .and_then(|code| self.concepts.get(code))
    }
    /// Structural source links, in source order; callers apply the declared hierarchy meaning.
    pub fn nested_relationships<'a>(
        &'a self,
        code: &str,
    ) -> impl Iterator<Item = (&'static str, &'a str)> + 'a {
        self.nested
            .get(code)
            .into_iter()
            .flat_map(|(parent, children)| {
                parent
                    .as_deref()
                    .map(|code| ("parent", code))
                    .into_iter()
                    .chain(children.iter().map(|code| ("child", code.as_str())))
            })
    }
    pub fn property_meaning<'a>(&'a self, code: &'a str) -> Option<&'a str> {
        self.definitions.meaning(code)
    }
    pub fn property_type(&self, code: &str) -> Option<&str> {
        self.definitions.kind(code)
    }
    pub fn boolean_property(&self, concept: &Concept, meaning: &str) -> bool {
        concept.property.iter().any(|property| {
            self.property_meaning(&property.code) == Some(meaning)
                && property
                    .value
                    .get("valueBoolean")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        })
    }
    pub fn ancestor_depths(&self, code: &str) -> Result<BTreeMap<String, usize>> {
        self.ancestor_depths_work(code, &mut 0)
    }
    fn ancestor_depths_work(
        &self,
        code: &str,
        work: &mut usize,
    ) -> Result<BTreeMap<String, usize>> {
        let mut depths = BTreeMap::new();
        let mut pending = std::collections::VecDeque::from([(code.to_owned(), 0)]);
        while let Some((c, depth)) = pending.pop_front() {
            if depth > MAX_DEPTH {
                return Err(Error("Hierarchy depth limit exceeded".into()));
            }
            for parent in self.parents.get(&c).into_iter().flatten() {
                *work += 1;
                if *work > graph::MAX_PREPARATION_WORK {
                    return Err(Error("Hierarchy traversal work limit exceeded".into()));
                }
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
/// Request-owned immutable preparations; they are never a global registry.
pub type SharedSystems = BTreeMap<Canonical, Arc<PreparedSystem>>;
const MAX_EVALUATION_WORK: usize = 10_000_000;
const MAX_SMALL_PLAN_BYTES: usize = 8 * 1024 * 1024;
#[derive(Default)]
struct Evaluation {
    work: usize,
    hierarchy_work: usize,
    regex_count: usize,
    regex_bytes: usize,
    ancestor_bytes: usize,
    memo: BTreeMap<Canonical, Members>,
    heights: BTreeMap<Canonical, usize>,
    memo_bytes: usize,
}
impl Evaluation {
    fn charge(&mut self, work: usize) -> Result<()> {
        self.work = self.work.saturating_add(work);
        if self.work > MAX_EVALUATION_WORK {
            return Err(Error("Finite expansion work limit exceeded".into()));
        }
        Ok(())
    }
    fn remember(&mut self, key: &Canonical, members: &Members) {
        if members.len() > 128 || self.memo.len() >= MAX_RESOURCES {
            return;
        }
        let bytes = 512
            + 4 * (key.url.len() + key.version.as_ref().map_or(0, String::len))
            + members
                .values()
                .map(|member| {
                    512 + 4
                        * (member.system.len()
                            + member.code.len()
                            + member.version.as_ref().map_or(0, String::len)
                            + member.display.as_ref().map_or(0, String::len))
                })
                .sum::<usize>();
        if self.memo_bytes.saturating_add(bytes) <= MAX_SMALL_PLAN_BYTES {
            self.memo.insert(key.clone(), members.clone());
            self.memo_bytes += bytes;
        }
    }
}
impl Snapshot {
    pub fn expand(&self, canonical: &Canonical) -> Result<Members> {
        self.expand_with_shared(canonical, &SharedSystems::new())
    }
    /// Use unchanged, caller-owned R4 preparations without retaining the shared view.
    pub fn expand_with_shared(
        &self,
        canonical: &Canonical,
        shared: &SharedSystems,
    ) -> Result<Members> {
        if !shared.is_empty() {
            if self
                .systems
                .len()
                .saturating_add(shared.len())
                .saturating_add(self.value_sets.len())
                > MAX_RESOURCES
            {
                return Err(Error("Shared snapshot resource limit exceeded".into()));
            }
            let mut concepts = self
                .systems
                .values()
                .map(|s| s.concepts.len())
                .sum::<usize>();
            for (key, system) in shared {
                if self.systems.contains_key(key)
                    || key.url != system.canonical.url
                    || key
                        .version
                        .as_ref()
                        .is_some_and(|version| Some(version) != system.canonical.version.as_ref())
                {
                    return Err(Error("Conflicting shared CodeSystem identity".into()));
                }
                concepts = concepts.saturating_add(system.concepts.len());
                if concepts > MAX_CONCEPTS {
                    return Err(Error("Shared snapshot concept limit exceeded".into()));
                }
                system.require_r4_composition()?;
            }
        }
        self.evaluate(
            canonical,
            &mut BTreeSet::new(),
            &mut Evaluation::default(),
            shared,
        )
    }
    fn evaluate(
        &self,
        canonical: &Canonical,
        path: &mut BTreeSet<Canonical>,
        evaluation: &mut Evaluation,
        shared: &SharedSystems,
    ) -> Result<Members> {
        evaluation.charge(1)?;
        if path.contains(canonical) {
            return Err(Error("ValueSet import cycle or depth limit".into()));
        }
        if let Some(members) = evaluation.memo.get(canonical) {
            if path.len() + evaluation.heights[canonical] > MAX_DEPTH {
                return Err(Error("ValueSet import cycle or depth limit".into()));
            }
            let members = members.clone();
            evaluation.charge(members.len())?;
            return Ok(members);
        }
        if path.len() >= MAX_DEPTH || !path.insert(canonical.clone()) {
            return Err(Error("ValueSet import cycle or depth limit".into()));
        }
        let compose = self
            .value_sets
            .get(canonical)
            .ok_or_else(|| Error(format!("Unresolved ValueSet {}", canonical.url)))?;
        let (_, imports) = compose.references()?;
        let mut result = Members::new();
        for selection in &compose.include {
            let included = self.select(selection, path, evaluation, shared)?;
            evaluation.charge(included.len())?;
            result.extend(included);
            if result.len() > MAX_CONCEPTS {
                return Err(Error("Expansion size limit exceeded".into()));
            }
        }
        for selection in &compose.exclude {
            let excluded = self.select(selection, path, evaluation, shared)?;
            evaluation.charge(result.len() + excluded.len())?;
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
        let height = 1 + imports
            .iter()
            .map(|key| evaluation.heights[key])
            .max()
            .unwrap_or(0);
        if !evaluation.heights.contains_key(canonical) && evaluation.heights.len() >= MAX_RESOURCES
        {
            return Err(Error("Finite expansion resource limit exceeded".into()));
        }
        evaluation.heights.insert(canonical.clone(), height);
        evaluation.remember(canonical, &result);
        Ok(result)
    }
    fn select(
        &self,
        selection: &Selection,
        path: &mut BTreeSet<Canonical>,
        evaluation: &mut Evaluation,
        shared: &SharedSystems,
    ) -> Result<Members> {
        evaluation.charge(1)?;
        let mut result: Option<Members> = None;
        if let Some(system) = &selection.system {
            let canonical = Canonical::parse(system, selection.version.as_deref())?;
            let system = self
                .systems
                .get(&canonical)
                .or_else(|| shared.get(&canonical).map(Arc::as_ref))
                .ok_or_else(|| Error(format!("Unresolved CodeSystem {}", canonical.url)))?;
            if system.explicit_projection
                && (selection.concept.is_empty() || !selection.filter.is_empty())
            {
                return Err(Error(
                    "A UCUM grammar projection requires explicit codes without filters; the grammar cannot be enumerated".into(),
                ));
            }
            if selection.filter.len() > 32 {
                return Err(Error(
                    "A finite selection supports at most 32 filters".into(),
                ));
            }
            let filters = selection
                .filter
                .iter()
                .map(|filter| filters::CompiledFilter::prepare(system, filter, evaluation))
                .collect::<Result<Vec<_>>>()?;
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
                evaluation.charge(1 + selection.filter.len())?;
                if !explicit.is_empty() && !explicit.contains_key(&normalize(code)) {
                    continue;
                }
                let mut matches = true;
                for filter in &filters {
                    matches &= filter.matches(system, concept, evaluation)?;
                }
                if !matches {
                    continue;
                }
                let inactive = system.boolean_property(concept, "inactive")
                    || concept.property.iter().any(|property| {
                        system.property_meaning(&property.code) == Some("status")
                            && property
                                .value
                                .get("valueCode")
                                .and_then(serde_json::Value::as_str)
                                == Some("retired")
                    });
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
                    abstract_code: system.boolean_property(concept, "notSelectable")
                        || system.boolean_property(concept, "abstract"),
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
            let members =
                self.evaluate(&Canonical::parse(imported, None)?, path, evaluation, shared)?;
            evaluation.charge(members.len() + result.as_ref().map_or(0, BTreeMap::len))?;
            if let Some(existing) = &mut result {
                existing.retain(|key, _| members.contains_key(key));
            } else {
                result = Some(members);
            }
        }
        Ok(result.unwrap_or_default())
    }
}
// Do not stringify Coding/decimal/date-time values into an invented comparison language.
fn equality_matches(property: &Property, expected: &str) -> Result<bool> {
    let (kind, value) = property.typed_value()?;
    Ok(match kind {
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
mod shared_tests;
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
        Ok(Self::from_members(snapshot.expand(canonical)?))
    }
    /// Build owned membership data from an immutable mixture of owned and shared sources.
    pub fn prepare_with_shared(
        snapshot: &Snapshot,
        canonical: &Canonical,
        shared: &SharedSystems,
    ) -> Result<Self> {
        Ok(Self::from_members(
            snapshot.expand_with_shared(canonical, shared)?,
        ))
    }
    fn from_members(members: Members) -> Self {
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
        Self { members, index }
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
