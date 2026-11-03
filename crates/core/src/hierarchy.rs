//! Immutable indexes over a provider's materialized transitive closure.
use crate::{Error, Result, MAX_CONCEPTS};
use std::{collections::HashMap, sync::Arc};

pub const MAX_HIERARCHY_RELATIONSHIPS: usize = 1_000_000;

/// The comparison key must use the same normalization as subsequent query keys.
/// SQL adapters can supply PostgreSQL lower() without substituting Rust case folding.
#[derive(Debug)]
pub struct HierarchyConcept {
    pub code: String,
    pub comparison_key: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HierarchyMatch<'a> {
    pub code_a: Option<&'a str>,
    pub code_b: Option<&'a str>,
    /// Strict relationship: true for A -> B, false for B -> A, None otherwise.
    /// Equivalence is represented by equal, non-missing stored codes.
    pub relationship: Option<bool>,
}

#[derive(Debug)]
pub struct PreparedHierarchy {
    case_sensitive: bool,
    codes: Vec<Arc<str>>,
    index: HashMap<Arc<str>, u32>,
    ancestors: Vec<Vec<u32>>,
}

impl PreparedHierarchy {
    /// Accept an already materialized closure, not a list of direct parent edges.
    pub fn from_closure(
        case_sensitive: bool,
        concepts: Vec<HierarchyConcept>,
        relationships: Vec<(String, String)>,
    ) -> Result<Self> {
        if concepts.len() > MAX_CONCEPTS || relationships.len() > MAX_HIERARCHY_RELATIONSHIPS {
            return Err(Error("Hierarchy preparation limit exceeded".into()));
        }
        let mut codes = Vec::with_capacity(concepts.len());
        let mut index = HashMap::with_capacity(concepts.len());
        let mut stored = HashMap::with_capacity(concepts.len());
        for concept in concepts {
            let id = codes.len() as u32;
            let code: Arc<str> = concept.code.into();
            let key: Arc<str> = if case_sensitive {
                if concept.comparison_key != code.as_ref() {
                    return Err(Error("Invalid case-sensitive comparison key".into()));
                }
                code.clone()
            } else {
                concept.comparison_key.into()
            };
            if stored.insert(code.clone(), id).is_some() || index.insert(key, id).is_some() {
                return Err(Error("Duplicate or ambiguous hierarchy code".into()));
            }
            codes.push(code);
        }
        let mut ancestors = vec![Vec::new(); codes.len()];
        for (ancestor, descendant) in relationships {
            let a = stored
                .get(ancestor.as_str())
                .ok_or_else(|| Error("Unknown closure ancestor".into()))?;
            let b = stored
                .get(descendant.as_str())
                .ok_or_else(|| Error("Unknown closure descendant".into()))?;
            if a != b {
                ancestors[*b as usize].push(*a);
            }
        }
        for entries in &mut ancestors {
            entries.sort_unstable();
            entries.dedup();
        }
        Ok(Self {
            case_sensitive,
            codes,
            index,
            ancestors,
        })
    }

    pub fn resolve<'a>(
        &'a self,
        code_a: &str,
        code_b: &str,
        comparison_a: &str,
        comparison_b: &str,
    ) -> HierarchyMatch<'a> {
        let a = self
            .index
            .get(if self.case_sensitive {
                code_a
            } else {
                comparison_a
            })
            .copied();
        let b = self
            .index
            .get(if self.case_sensitive {
                code_b
            } else {
                comparison_b
            })
            .copied();
        let relationship = match (a, b) {
            (Some(a), Some(b)) if a != b => {
                if self.ancestors[b as usize].binary_search(&a).is_ok() {
                    Some(true)
                } else if self.ancestors[a as usize].binary_search(&b).is_ok() {
                    Some(false)
                } else {
                    None
                }
            }
            _ => None,
        };
        HierarchyMatch {
            code_a: a.map(|id| self.codes[id as usize].as_ref()),
            code_b: b.map(|id| self.codes[id as usize].as_ref()),
            relationship,
        }
    }

    /// Conservative retained allocation charge, not a hard process RSS bound.
    pub fn memory_charge(&self) -> usize {
        4096 + self.codes.capacity() * std::mem::size_of::<Arc<str>>()
            + self.index.capacity() * 64
            + self.ancestors.capacity() * std::mem::size_of::<Vec<u32>>()
            + self.codes.iter().map(|s| s.len() + 32).sum::<usize>()
            + self.index.keys().map(|s| s.len() + 32).sum::<usize>()
            + self
                .ancestors
                .iter()
                .map(|v| v.capacity() * std::mem::size_of::<u32>())
                .sum::<usize>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn concept(code: &str, key: &str) -> HierarchyConcept {
        HierarchyConcept {
            code: code.into(),
            comparison_key: key.into(),
        }
    }

    #[test]
    fn materialized_closure_resolves_all_outcomes_and_missing_codes() {
        let plan = PreparedHierarchy::from_closure(
            true,
            ["root", "middle", "leaf", "sibling"]
                .into_iter()
                .map(|c| concept(c, c))
                .collect(),
            [
                ("root", "middle"),
                ("middle", "leaf"),
                ("root", "leaf"),
                ("root", "sibling"),
            ]
            .into_iter()
            .map(|(a, b)| (a.into(), b.into()))
            .collect(),
        )
        .unwrap();
        assert_eq!(
            plan.resolve("root", "leaf", "", "").relationship,
            Some(true)
        );
        assert_eq!(
            plan.resolve("leaf", "root", "", "").relationship,
            Some(false)
        );
        assert_eq!(plan.resolve("leaf", "sibling", "", "").relationship, None);
        let same = plan.resolve("leaf", "leaf", "", "");
        assert_eq!(same.code_a, same.code_b);
        assert_eq!(same.relationship, None);
        let missing = plan.resolve("absent", "leaf", "", "");
        assert_eq!(missing.code_a, None);
        assert_eq!(missing.code_b, Some("leaf"));
        assert_eq!(missing.relationship, None);
        assert_eq!(plan.resolve("ROOT", "leaf", "root", "leaf").code_a, None);
    }

    #[test]
    fn comparison_keys_are_provider_defined_and_stored_spelling_survives() {
        let plan = PreparedHierarchy::from_closure(
            false,
            vec![concept("Parent", "db-parent"), concept("İ", "db-i")],
            vec![("Parent".into(), "İ".into())],
        )
        .unwrap();
        let found = plan.resolve("arbitrary provider spelling", "i", "db-parent", "db-i");
        assert_eq!(found.code_a, Some("Parent"));
        assert_eq!(found.code_b, Some("İ"));
        assert_eq!(found.relationship, Some(true));
        let same = plan.resolve("İ", "i", "db-i", "db-i");
        assert_eq!(same.code_a, same.code_b);
        assert_eq!(same.relationship, None);
        assert_eq!(plan.resolve("Parent", "İ", "parent", "i").code_a, None);
    }

    #[test]
    fn invalid_or_ambiguous_projection_is_rejected() {
        assert!(
            PreparedHierarchy::from_closure(true, vec![concept("a", "wrong")], vec![]).is_err()
        );
        assert!(PreparedHierarchy::from_closure(
            true,
            vec![concept("a", "a"), concept("a", "a")],
            vec![]
        )
        .is_err());
        assert!(PreparedHierarchy::from_closure(
            false,
            vec![concept("a", "a"), concept("A", "a")],
            vec![]
        )
        .is_err());
        for edge in [("unknown", "a"), ("a", "unknown")] {
            assert!(PreparedHierarchy::from_closure(
                true,
                vec![concept("a", "a")],
                vec![(edge.0.into(), edge.1.into())]
            )
            .is_err());
        }
    }

    #[test]
    fn repeated_and_self_edges_preserve_sql_equivalence_and_direction_precedence() {
        let plan = PreparedHierarchy::from_closure(
            true,
            vec![concept("a", "a"), concept("b", "b")],
            [("a", "a"), ("a", "b"), ("a", "b"), ("b", "a")]
                .into_iter()
                .map(|(a, b)| (a.into(), b.into()))
                .collect(),
        )
        .unwrap();
        assert_eq!(plan.resolve("a", "a", "", "").relationship, None);
        assert_eq!(plan.resolve("a", "b", "", "").relationship, Some(true));
        assert_eq!(plan.resolve("b", "a", "", "").relationship, Some(true));
    }

    #[test]
    fn empty_and_retained_plans_have_independent_owned_storage() {
        let empty = PreparedHierarchy::from_closure(true, vec![], vec![]).unwrap();
        assert_eq!(empty.resolve("a", "b", "", "").code_a, None);
        let input = vec![concept("a", "a"), concept("b", "b")];
        let plan = Arc::new(
            PreparedHierarchy::from_closure(true, input, vec![("a".into(), "b".into())]).unwrap(),
        );
        let reader = plan.clone();
        drop(plan);
        assert_eq!(reader.resolve("a", "b", "", "").relationship, Some(true));
        assert!(reader.memory_charge() > empty.memory_charge());
    }

    #[test]
    fn preparation_limits_are_checked_before_building_indexes() {
        let concepts = (0..=MAX_CONCEPTS).map(|_| concept("", "")).collect();
        assert!(PreparedHierarchy::from_closure(true, concepts, vec![]).is_err());
        let edges = vec![(String::new(), String::new()); MAX_HIERARCHY_RELATIONSHIPS + 1];
        assert!(PreparedHierarchy::from_closure(true, vec![], edges).is_err());
    }
}
