//! Operation-owned finite leaves for the projection-backed compose graph.
//! Sources and compact knowledge are bounded; nothing is published or retained globally.
use super::{compose_loader, ExpansionOptions, ValueSetOperation};
use crate::{error::AppError, models::ExpansionSources};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use term_squid_core::{Canonical, CodeSystemIndex, Member, MemberKey, Selection};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

const SOURCE_BYTES: i64 = 32 * 1024 * 1024;
const OUTPUT_CHARGE: usize = 32 * 1024 * 1024;

#[cfg(test)]
type PauseWait = (
    tokio::sync::oneshot::Sender<()>,
    std::sync::mpsc::Receiver<()>,
);

/// A test-local pause belongs to one scoped operation, never a global request hook.
#[cfg(test)]
pub(crate) struct Pause {
    pending: std::sync::Mutex<Option<PauseWait>>,
}
#[cfg(test)]
impl Pause {
    pub fn new() -> (
        Self,
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (started, entered) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        (
            Self {
                pending: std::sync::Mutex::new(Some((started, released))),
            },
            entered,
            release,
        )
    }

    fn wait(&self) {
        let pending = self.pending.lock().expect("Finite CPU pause lock").take();
        if let Some((started, release)) = pending {
            let _ = started.send(());
            // Dropping the test's sender also releases the worker during assertion unwind.
            let _ = release.recv();
        }
    }
}
#[cfg(test)]
tokio::task_local! { static CPU_PAUSE: std::sync::Arc<Pause>; }

#[cfg(test)]
pub(crate) async fn paused<F: std::future::Future>(pause: Pause, future: F) -> F::Output {
    CPU_PAUSE.scope(std::sync::Arc::new(pause), future).await
}

fn limit(message: &str) -> AppError {
    AppError::Terminology(term_squid_core::Error(message.into()))
}

#[derive(Debug, Serialize)]
pub(super) struct SystemRow {
    pub id: Uuid,
    pub url: String,
    pub version: Option<String>,
    pub case_sensitive: bool,
}

pub(super) struct SelectionRows {
    pub id: Uuid,
    pub selected: Canonical,
    pub rows: Value,
}

#[derive(Debug)]
pub(crate) struct Metadata {
    pub primary: Option<String>,
    pub designations: Vec<Value>,
    pub parent: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Page {
    pub metadata: BTreeMap<MemberKey, Metadata>,
    pub knowledge: BTreeMap<Canonical, CodeSystemIndex>,
}
impl Page {
    pub fn get(&self, member: &Member) -> Option<&Metadata> {
        self.metadata.get(&key(member))
    }
}

pub(super) struct Bridge {
    pub systems: BTreeMap<Canonical, SystemRow>,
    pub sources: ExpansionSources,
    remaining_source_bytes: i64,
    concepts: usize,
    rows: usize,
    charge: usize,
    metadata: BTreeMap<MemberKey, Metadata>,
    knowledge: BTreeMap<Canonical, CodeSystemIndex>,
    ucum_codes: BTreeSet<String>,
}
impl Default for Bridge {
    fn default() -> Self {
        Self {
            systems: BTreeMap::new(),
            sources: ExpansionSources::default(),
            remaining_source_bytes: SOURCE_BYTES,
            concepts: 0,
            rows: 0,
            charge: 0,
            metadata: BTreeMap::new(),
            knowledge: BTreeMap::new(),
            ucum_codes: BTreeSet::new(),
        }
    }
}
impl Bridge {
    pub async fn load(
        &mut self,
        tx: &mut Transaction<'_, Postgres>,
        selection: &Selection,
        context: Option<(&str, &str)>,
        options: Option<&ExpansionOptions>,
        operation: ValueSetOperation<'_>,
        permit: &mut Option<OwnedSemaphorePermit>,
    ) -> Result<SelectionRows, AppError> {
        let loaded = compose_loader::load_selection(
            tx,
            selection,
            context,
            options,
            &mut self.remaining_source_bytes,
        )
        .await
        .map_err(|error| match error {
            AppError::NativeValueSet => {
                limit("Hybrid composition encountered an unsupported native CodeSystem provider")
            }
            other => other,
        })?;
        let remaining_concepts = term_squid_core::MAX_CONCEPTS.saturating_sub(self.concepts);
        let remaining_rows = term_squid_core::MAX_CONCEPTS.saturating_sub(self.rows);
        let remaining_charge = OUTPUT_CHARGE.saturating_sub(self.charge);
        let needle = match operation {
            ValueSetOperation::Find { code, .. } => Some(code.to_owned()),
            _ => None,
        };
        let filter = match operation {
            ValueSetOperation::Expand { filter, .. } => filter.map(str::to_lowercase),
            _ => None,
        };
        let language = options.and_then(|options| options.display_language.clone());
        let ucum_codes = self.ucum_codes.clone();
        let work = permit.take().ok_or(AppError::Overloaded)?;
        #[cfg(test)]
        let pause = CPU_PAUSE.try_with(std::sync::Arc::clone).ok();
        // Cancellation cannot release CPU admission while the blocking leaf still executes.
        let (prepared, work) = tokio::task::spawn_blocking(move || {
            #[cfg(test)]
            if let Some(pause) = pause {
                pause.wait();
            }
            let result = prepare(
                loaded,
                needle.as_deref(),
                filter.as_deref(),
                language.as_deref(),
                ucum_codes,
                remaining_concepts,
                remaining_rows,
                remaining_charge,
            );
            (result, work)
        })
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
        *permit = Some(work);
        let prepared = prepared?;
        self.concepts += prepared.concepts;
        self.rows += prepared.rows.len();
        self.charge += prepared.charge;
        self.metadata.extend(prepared.metadata);
        for (canonical, knowledge) in prepared.knowledge {
            if canonical.url == term_squid_core::ucum::SYSTEM {
                self.knowledge.insert(canonical, knowledge);
            } else {
                self.knowledge.entry(canonical).or_insert(knowledge);
            }
        }
        self.ucum_codes = prepared.ucum_codes;
        self.sources.systems.extend(prepared.sources.systems);
        self.sources
            .supplements
            .extend(prepared.sources.supplements);
        // A finite leaf has no imported sets. In particular its private synthetic root
        // must never appear among the user's dependency/provenance parameters.
        let selected = prepared.selected;
        let system = self
            .systems
            .entry(selected.clone())
            .or_insert_with(|| SystemRow {
                id: Uuid::new_v4(),
                url: selected.url.clone(),
                version: selected.version.clone(),
                case_sensitive: prepared.case_sensitive,
            });
        Ok(SelectionRows {
            id: system.id,
            selected,
            rows: prepared.rows.into(),
        })
    }

    pub fn finish(mut self, members: &[Member]) -> Page {
        let returned: BTreeSet<_> = members.iter().map(key).collect();
        self.metadata.retain(|key, _| returned.contains(key));
        Page {
            metadata: self.metadata,
            knowledge: self.knowledge,
        }
    }
}

fn key(member: &Member) -> MemberKey {
    MemberKey {
        system: member.system.clone(),
        version: member.version.clone(),
        code: member.code.clone(),
    }
}

struct Prepared {
    selected: Canonical,
    case_sensitive: bool,
    rows: Vec<Value>,
    metadata: BTreeMap<MemberKey, Metadata>,
    knowledge: BTreeMap<Canonical, CodeSystemIndex>,
    sources: ExpansionSources,
    concepts: usize,
    charge: usize,
    ucum_codes: BTreeSet<String>,
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    loaded: compose_loader::Loaded,
    needle: Option<&str>,
    filter: Option<&str>,
    language: Option<&str>,
    mut ucum_codes: BTreeSet<String>,
    remaining_concepts: usize,
    remaining_rows: usize,
    remaining_charge: usize,
) -> Result<Prepared, AppError> {
    let concepts = compose_loader::source_concept_count(&loaded)?;
    if concepts > remaining_concepts {
        return Err(limit(
            "Hybrid finite sources exceed 100000 aggregate concepts",
        ));
    }
    let (expanded, resources) = compose_loader::prepare_rich(loaded)?;
    let selected = expanded
        .sources
        .systems
        .first()
        .filter(|_| expanded.sources.systems.len() == 1)
        .cloned()
        .ok_or_else(|| limit("A hybrid finite leaf must resolve exactly one CodeSystem"))?;
    let source = resources
        .values()
        .find(|source| {
            source["url"].as_str() == Some(selected.url.as_str())
                && source["version"].as_str() == selected.version.as_deref()
        })
        .ok_or_else(|| limit("Hybrid finite source metadata is unavailable"))?;
    let case_sensitive = source["caseSensitive"].as_bool().unwrap_or(true);
    let mut index = BTreeMap::new();
    let mut pending = source["concept"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|concept| (concept, None))
        .collect::<Vec<_>>();
    while let Some((concept, parent)) = pending.pop() {
        let code = concept["code"]
            .as_str()
            .ok_or_else(|| limit("Hybrid finite concept code is unavailable"))?;
        index.insert(code, (concept, parent));
        pending.extend(
            concept["concept"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|child| (child, Some(code))),
        );
    }
    let crate::models::ExpandedValueSet {
        members,
        sources,
        knowledge,
        ..
    } = expanded;
    let mut knowledge = std::sync::Arc::try_unwrap(knowledge)
        .map_err(|_| limit("Hybrid finite knowledge must be operation-owned"))?;
    if selected.url == term_squid_core::ucum::SYSTEM {
        ucum_codes.extend(index.keys().map(|code| (*code).to_owned()));
        if ucum_codes.len() > term_squid_core::ucum::MAX_EXPLICIT_CODES {
            return Err(limit(
                "Hybrid UCUM exceeds 1024 distinct explicit expressions",
            ));
        }
        // Different explicit leaves may project different parts of the same grammar.
        // Preserve their positive knowledge without declaring the grammar enumerable.
        let definition = term_squid_core::CodeSystem::deserialize(&json!({
            "url":selected.url,"version":selected.version,"content":"fragment",
            "caseSensitive":true,"concept":ucum_codes.iter().map(|code| json!({"code":code})).collect::<Vec<_>>()
        }))
        .map_err(|error| AppError::Internal(error.into()))?;
        knowledge.insert(
            selected.clone(),
            term_squid_core::PreparedSystem::prepare_for_r4_source(definition)?.into_code_index(),
        );
    }
    let mut charge = knowledge
        .values()
        .map(CodeSystemIndex::memory_charge)
        .sum::<usize>();
    if charge > remaining_charge {
        return Err(limit("Hybrid finite bridge exceeds 32 MiB output budget"));
    }
    let mut rows = Vec::new();
    let mut metadata = BTreeMap::new();
    for member in members.values() {
        if rows.len() >= remaining_rows {
            return Err(limit("Hybrid finite bridge exceeds 100000 aggregate rows"));
        }
        let (concept, parent) = index
            .get(member.code.as_str())
            .ok_or_else(|| limit("Hybrid finite member metadata is unavailable"))?;
        let designations = concept["designation"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if designations.len() > 256 {
            return Err(limit(
                "Hybrid finite concept designations exceed 256 entries",
            ));
        }
        let localized =
            language.and_then(|language| super::expansion::preferred(&designations, language));
        let display = localized.or(member.display.as_deref()).map(str::to_owned);
        let post_coordinated = member.system == term_squid_core::snomed::SYSTEM
            && member
                .code
                .chars()
                .any(|character| matches!(character, ':' | '+'))
            || concept["property"].as_array().is_some_and(|properties| {
                properties.iter().any(|property| {
                    property["code"] == "postCoordinated" && property["valueBoolean"] == true
                })
            });
        let item = Metadata {
            primary: concept["display"].as_str().map(str::to_owned),
            designations,
            parent: parent.map(str::to_owned),
        };
        let row = json!({
            "code":member.code,"display":display,"inactive":member.inactive,
            "abstract_code":member.abstract_code,"post_coordinated":post_coordinated,
            "membership_match":needle.is_none_or(|needle| member.code == needle || !member.case_sensitive && member.code.to_lowercase() == needle.to_lowercase()),
            "text_match":filter.is_none_or(|filter| member.code.to_lowercase().contains(filter) || display.as_deref().is_some_and(|display| display.to_lowercase().contains(filter)))
        });
        let bytes = serde_json::to_vec(&row)
            .map_err(|error| AppError::Internal(error.into()))?
            .len()
            + serde_json::to_vec(&item.designations)
                .map_err(|error| AppError::Internal(error.into()))?
                .len()
            + item.primary.as_ref().map_or(0, String::len)
            + item.parent.as_ref().map_or(0, String::len)
            + member.system.len()
            + member.version.as_ref().map_or(0, String::len);
        charge = charge.saturating_add(1024_usize.saturating_add(bytes.saturating_mul(8)));
        if charge > remaining_charge {
            return Err(limit("Hybrid finite bridge exceeds 32 MiB output budget"));
        }
        rows.push(row);
        metadata.insert(key(member), item);
    }
    Ok(Prepared {
        selected,
        case_sensitive,
        rows,
        metadata,
        knowledge,
        sources: (*sources).clone(),
        concepts,
        charge,
        ucum_codes,
    })
}
