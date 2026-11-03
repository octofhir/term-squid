//! Immutable concept payloads indexed by provider-defined comparison keys.
use crate::{Error, Result, MAX_CONCEPTS};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct ProjectedConcept {
    pub code: String,
    pub display: Option<String>,
    pub definition: Option<String>,
    pub properties: Option<Value>,
}
impl ProjectedConcept {
    /// Conservative retained allocation charge, not a process RSS bound.
    pub fn memory_charge(&self) -> usize {
        let mut charge = std::mem::size_of::<Self>()
            + self.code.capacity()
            + self.display.as_ref().map_or(0, String::capacity)
            + self.definition.as_ref().map_or(0, String::capacity);
        let mut pending: Vec<_> = self.properties.iter().collect();
        while let Some(value) = pending.pop() {
            charge += std::mem::size_of::<Value>();
            match value {
                Value::String(s) => charge += s.capacity(),
                Value::Array(values) => {
                    charge += values.capacity() * std::mem::size_of::<Value>();
                    pending.extend(values);
                }
                Value::Object(values) => {
                    charge += values.len() * 128;
                    for (key, value) in values {
                        charge += key.capacity();
                        pending.push(value);
                    }
                }
                _ => {}
            }
        }
        charge
    }
}

#[derive(Debug)]
pub struct PreparedConcepts {
    case_sensitive: bool,
    index: HashMap<String, ProjectedConcept>,
}
impl PreparedConcepts {
    /// Each row pairs its provider-normalized key with the unchanged stored payload.
    pub fn from_rows(case_sensitive: bool, rows: Vec<(String, ProjectedConcept)>) -> Result<Self> {
        if rows.len() > MAX_CONCEPTS {
            return Err(Error("Concept preparation limit exceeded".into()));
        }
        let mut index = HashMap::with_capacity(rows.len());
        let mut stored = HashSet::with_capacity(rows.len());
        for (key, concept) in rows {
            if case_sensitive && key != concept.code {
                return Err(Error("Invalid case-sensitive comparison key".into()));
            }
            if !stored.insert(concept.code.clone()) || index.insert(key, concept).is_some() {
                return Err(Error("Duplicate or ambiguous concept code".into()));
            }
        }
        Ok(Self {
            case_sensitive,
            index,
        })
    }
    pub fn resolve(&self, code: &str, comparison_key: &str) -> Option<&ProjectedConcept> {
        self.index.get(if self.case_sensitive {
            code
        } else {
            comparison_key
        })
    }
    pub fn memory_charge(&self) -> usize {
        4096 + self.index.capacity() * (std::mem::size_of::<ProjectedConcept>() + 64)
            + self
                .index
                .iter()
                .map(|(key, value)| key.capacity() + value.memory_charge())
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn row(code: &str, key: &str) -> (String, ProjectedConcept) {
        (
            key.into(),
            ProjectedConcept {
                code: code.into(),
                display: Some("Shown".into()),
                definition: Some("Definition".into()),
                properties: Some(json!({"number": 7,"flag":true,"nested":[null,"raw"]})),
            },
        )
    }
    #[test]
    fn exact_missing_and_provider_folded_keys_preserve_payloads() {
        let exact = PreparedConcepts::from_rows(true, vec![row("A", "A")]).unwrap();
        assert!(exact.resolve("a", "A").is_none());
        assert!(exact.resolve("missing", "A").is_none());
        let found = exact.resolve("A", "ignored").unwrap();
        assert_eq!(found.display.as_deref(), Some("Shown"));
        assert_eq!(found.definition.as_deref(), Some("Definition"));
        assert_eq!(
            found.properties.as_ref().unwrap(),
            &json!({"number":7,"flag":true,"nested":[null,"raw"]})
        );
        let folded = PreparedConcepts::from_rows(false, vec![row("İ", "provider-key")]).unwrap();
        assert_eq!(
            folded
                .resolve("arbitrary query spelling", "provider-key")
                .unwrap()
                .code,
            "İ"
        );
        assert!(folded
            .resolve("İ", "rust-lowercase-is-not-the-provider")
            .is_none());
    }
    #[test]
    fn duplicate_stored_codes_ambiguous_keys_and_invalid_exact_keys_fail() {
        for rows in [vec![row("A", "wrong")], vec![row("A", "A"), row("A", "A")]] {
            assert!(PreparedConcepts::from_rows(true, rows).is_err());
        }
        for rows in [
            vec![row("A", "same"), row("a", "same")],
            vec![row("A", "first"), row("A", "second")],
        ] {
            assert!(PreparedConcepts::from_rows(false, rows).is_err());
        }
    }
    #[test]
    fn preparation_counts_are_bounded_before_indexing() {
        let rows = (0..=MAX_CONCEPTS)
            .map(|_| {
                (
                    String::new(),
                    ProjectedConcept {
                        code: String::new(),
                        display: None,
                        definition: None,
                        properties: None,
                    },
                )
            })
            .collect();
        assert!(PreparedConcepts::from_rows(true, rows).is_err());
    }
    #[test]
    fn charges_include_nested_json_and_large_payload_buffers() {
        let (_, mut value) = row("A", "A");
        let initial = value.memory_charge();
        value.properties = Some(json!({"array":vec![json!({});2000],"text":"x".repeat(4096)}));
        assert!(value.memory_charge() > initial + 4096);
        let charge = value.memory_charge();
        let plan = PreparedConcepts::from_rows(true, vec![("A".into(), value)]).unwrap();
        assert!(plan.memory_charge() > charge);
        assert_eq!(
            plan.resolve("A", "").unwrap().properties.as_ref().unwrap()["array"]
                .as_array()
                .unwrap()
                .len(),
            2000
        );
    }
    #[test]
    fn empty_indexes_and_retained_immutable_ownership_are_independent() {
        let empty = PreparedConcepts::from_rows(true, vec![]).unwrap();
        assert!(empty.resolve("A", "").is_none());
        let plan =
            std::sync::Arc::new(PreparedConcepts::from_rows(true, vec![row("A", "A")]).unwrap());
        let retained = plan.clone();
        drop(plan);
        assert_eq!(retained.resolve("A", "").unwrap().code, "A");
        assert!(retained.memory_charge() > empty.memory_charge());
    }
}
