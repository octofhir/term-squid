//! Immutable R4 ConceptMap indexes. No HTTP or database dependencies.
use crate::{Error, Result, MAX_CONCEPTS, MAX_RESOURCES};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Default)]
pub struct PreparedConceptMap {
    groups: Vec<Group>,
    charge: usize,
}
struct Group {
    source: Option<Arc<str>>,
    target: Option<Arc<str>>,
    mappings: Vec<Mapping>,
    forward: BTreeMap<Arc<str>, Vec<usize>>,
    reverse: BTreeMap<Arc<str>, Vec<usize>>,
    conditional_codes: BTreeSet<Arc<str>>,
    reverse_conditional: bool,
    unmapped: bool,
}
struct Mapping {
    source: Arc<str>,
    source_display: Option<Arc<str>>,
    target: Arc<str>,
    target_display: Option<Arc<str>>,
    equivalence: Arc<str>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct TranslationMatch<'a> {
    pub system: &'a str,
    pub code: &'a str,
    pub display: Option<&'a str>,
    pub equivalence: &'a str,
}
impl PreparedConceptMap {
    pub fn prepare(content: &Value) -> Result<Self> {
        let mut plan = Self::default();
        let mut edges = 0;
        for group in content
            .get("group")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if plan.groups.len() >= MAX_RESOURCES {
                return Err(Error("ConceptMap group limit exceeded".into()));
            }
            let mut prepared = Group {
                source: text(group, "source", &mut plan.charge),
                target: text(group, "target", &mut plan.charge),
                mappings: Vec::new(),
                forward: BTreeMap::new(),
                reverse: BTreeMap::new(),
                conditional_codes: BTreeSet::new(),
                reverse_conditional: false,
                unmapped: group.get("unmapped").is_some(),
            };
            plan.charge += 4096;
            for element in group
                .get("element")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let source = text(element, "code", &mut plan.charge);
                let source_display = text(element, "display", &mut plan.charge);
                for target in element
                    .get("target")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    edges += 1;
                    if edges > MAX_CONCEPTS {
                        return Err(Error("ConceptMap mapping limit exceeded".into()));
                    }
                    plan.charge += 1024;
                    // Keep the adapter's error scope, including reverse conditional groups.
                    if target.get("dependsOn").is_some() || target.get("product").is_some() {
                        prepared.reverse_conditional = true;
                        if let Some(code) = &source {
                            prepared.conditional_codes.insert(code.clone());
                        }
                        continue;
                    }
                    let equivalence = target
                        .get("equivalence")
                        .and_then(Value::as_str)
                        .unwrap_or("equivalent");
                    if matches!(equivalence, "unmatched" | "disjoint") {
                        continue;
                    }
                    let Some(source) = &source else {
                        continue;
                    };
                    let Some(target_code) = text(target, "code", &mut plan.charge) else {
                        continue;
                    };
                    let index = prepared.mappings.len();
                    prepared
                        .forward
                        .entry(source.clone())
                        .or_default()
                        .push(index);
                    prepared
                        .reverse
                        .entry(target_code.clone())
                        .or_default()
                        .push(index);
                    plan.charge += equivalence.len() * 8;
                    prepared.mappings.push(Mapping {
                        source: source.clone(),
                        source_display: source_display.clone(),
                        target: target_code,
                        target_display: text(target, "display", &mut plan.charge),
                        equivalence: Arc::from(equivalence),
                    });
                }
            }
            plan.groups.push(prepared);
        }
        Ok(plan)
    }
    pub fn translate<'a>(
        &'a self,
        system: &str,
        code: &str,
        target_system: Option<&str>,
        reverse: bool,
    ) -> Result<Vec<TranslationMatch<'a>>> {
        let mut matches = Vec::new();
        for group in &self.groups {
            let input_system = if reverse {
                &group.target
            } else {
                &group.source
            };
            if input_system.as_deref() != Some(system) {
                continue;
            }
            if (reverse && group.reverse_conditional)
                || (!reverse && group.conditional_codes.contains(code))
            {
                return Err(Error("Conditional mappings are not supported".into()));
            }
            let output_system = if reverse {
                &group.source
            } else {
                &group.target
            };
            let output_system = output_system.as_deref().unwrap_or("");
            if target_system.is_some_and(|target| target != output_system) {
                continue;
            }
            let index = if reverse {
                &group.reverse
            } else {
                &group.forward
            };
            if !index.contains_key(code) && group.unmapped {
                return Err(Error("ConceptMap unmapped modes are not supported".into()));
            }
            for entry in index.get(code).into_iter().flatten() {
                let mapping = &group.mappings[*entry];
                matches.push(TranslationMatch {
                    system: output_system,
                    code: if reverse {
                        &mapping.source
                    } else {
                        &mapping.target
                    },
                    display: if reverse {
                        mapping.source_display.as_deref()
                    } else {
                        mapping.target_display.as_deref()
                    },
                    equivalence: if reverse {
                        reverse_equivalence(&mapping.equivalence)
                    } else {
                        &mapping.equivalence
                    },
                });
            }
        }
        Ok(matches)
    }
    /// Conservative retained-memory charge, including index nodes and shared string buffers.
    pub fn memory_charge(&self) -> usize {
        self.charge
    }
}
fn text(value: &Value, key: &str, charge: &mut usize) -> Option<Arc<str>> {
    value.get(key).and_then(Value::as_str).map(|text| {
        *charge += 128 + text.len() * 8;
        Arc::from(text)
    })
}
fn reverse_equivalence(value: &str) -> &str {
    match value {
        "wider" => "narrower",
        "narrower" => "wider",
        "subsumes" => "specializes",
        "specializes" => "subsumes",
        value => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn indexes_preserve_order_duplicates_reverse_display_and_equivalence() {
        let plan=PreparedConceptMap::prepare(&json!({"group":[{"source":"urn:s","target":"urn:t","element":[
            {"code":"a","display":"A","target":[{"code":"b","display":"B","equivalence":"wider"},{"code":"c"},{"code":"b"}]}]}]})).unwrap();
        let matches = plan.translate("urn:s", "a", Some("urn:t"), false).unwrap();
        assert_eq!(
            matches.iter().map(|m| m.code).collect::<Vec<_>>(),
            vec!["b", "c", "b"]
        );
        let reverse = plan.translate("urn:t", "b", Some("urn:s"), true).unwrap();
        assert_eq!(reverse.len(), 2);
        assert_eq!(
            reverse[0],
            TranslationMatch {
                system: "urn:s",
                code: "a",
                display: Some("A"),
                equivalence: "narrower"
            }
        );
        assert!(plan
            .translate("urn:s", "missing", None, false)
            .unwrap()
            .is_empty());
        assert!(plan
            .translate("urn:s", "a", Some("urn:wrong"), false)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn disjoint_and_unmatched_do_not_become_positive_or_reverse_matches() {
        let plan=PreparedConceptMap::prepare(&json!({"group":[{"source":"s","target":"t","element":[{"code":"a","target":[{"code":"b","equivalence":"disjoint"},{"code":"c","equivalence":"unmatched"}]}]}]})).unwrap();
        assert!(plan.translate("s", "a", None, false).unwrap().is_empty());
        assert!(plan.translate("t", "b", None, true).unwrap().is_empty());
    }
    #[test]
    fn conditional_error_scope_and_unmapped_modes_are_explicit() {
        let plan=PreparedConceptMap::prepare(&json!({"group":[{"source":"s","target":"t","element":[{"code":"a","target":[{"code":"b","dependsOn":[]}]},{"code":"c","target":[{"code":"d"}]}]}]})).unwrap();
        assert!(plan.translate("s", "a", Some("other"), false).is_err());
        assert_eq!(plan.translate("s", "c", None, false).unwrap()[0].code, "d");
        assert!(plan.translate("t", "d", None, true).is_err());
        let plan = PreparedConceptMap::prepare(
            &json!({"group":[{"source":"s","target":"t","unmapped":{"mode":"provided"}}]}),
        )
        .unwrap();
        assert!(plan.translate("s", "a", None, false).is_err());
    }
}
