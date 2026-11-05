//! Compile bounded R4 filters once per selection; no backtracking expression engine.
use crate::{equality_matches, Concept, Error, Evaluation, Filter, PreparedSystem, Result};
use regex_automata::{meta::Regex, util::syntax};

pub(super) struct CompiledFilter<'a> {
    definition: &'a Filter,
    expression: Option<Regex>,
    ancestors: Option<std::collections::BTreeSet<String>>,
    anchor: Option<String>,
    codes: Option<std::collections::BTreeSet<String>>,
}
impl<'a> CompiledFilter<'a> {
    pub(super) fn prepare(
        system: &PreparedSystem,
        filter: &'a Filter,
        work: &mut Evaluation,
    ) -> Result<Self> {
        let boolean = system.r4_semantics
            && matches!(
                system.property_meaning(&filter.property),
                Some("inactive" | "notSelectable" | "abstract")
            );
        let hierarchy = filter.property == "concept"
            && matches!(
                filter.op.as_str(),
                "is-a" | "descendent-of" | "is-not-a" | "child-of" | "generalizes"
            );
        let extended = system.r4_semantics
            && (matches!(filter.op.as_str(), "in" | "not-in" | "regex")
                || filter.property == "concept"
                    && matches!(filter.op.as_str(), "child-of" | "generalizes"));
        if !(matches!(filter.op.as_str(), "=" | "exists")
            || hierarchy && matches!(filter.op.as_str(), "is-a" | "descendent-of" | "is-not-a")
            || extended)
        {
            return Err(Error(format!(
                "Unsupported filter {} {}",
                filter.property, filter.op
            )));
        }
        if filter.op == "exists" && !matches!(filter.value.as_str(), "true" | "false") {
            return Err(Error("exists expects true or false".into()));
        }
        let anchor = if hierarchy {
            if system.r4_semantics {
                system.canonical_code(&filter.value).map(str::to_owned)
            } else {
                system
                    .concepts
                    .contains_key(&filter.value)
                    .then(|| filter.value.clone())
            }
        } else {
            None
        };
        if hierarchy {
            if system.hierarchy_meaning.as_deref() != Some("is-a") {
                return Err(Error("Hierarchy is not declared as is-a".into()));
            }
            if anchor.is_none() {
                return Err(Error(format!("Unknown hierarchy code {}", filter.value)));
            }
        } else if filter.property != "code"
            && filter.property != "concept"
            && !boolean
            && !system.properties.contains(&filter.property)
            && !system.concepts.values().any(|concept| {
                concept
                    .property
                    .iter()
                    .any(|property| property.code == filter.property)
            })
        {
            return Err(Error(format!(
                "Unknown filter property {}",
                filter.property
            )));
        }
        if matches!(filter.op.as_str(), "in" | "not-in")
            && (filter.value.len() > 4096
                || filter.value.split(',').count() > 256
                || filter.value.split(',').any(str::is_empty))
        {
            return Err(Error(
                "Membership filters require 1..256 nonempty scalar values within 4096 bytes".into(),
            ));
        }
        if matches!(filter.op.as_str(), "=" | "in" | "not-in")
            && (boolean || system.property_type(&filter.property) == Some("boolean"))
        {
            for value in if filter.op == "=" {
                vec![filter.value.as_str()]
            } else {
                filter.value.split(',').collect()
            } {
                value
                    .parse::<bool>()
                    .map_err(|_| Error("Boolean filters require true or false".into()))?;
            }
        }
        if matches!(filter.op.as_str(), "=" | "in" | "not-in")
            && system
                .property_type(&filter.property)
                .is_some_and(|kind| !matches!(kind, "code" | "string" | "boolean" | "integer"))
        {
            return Err(Error(
                "This property type does not support scalar equality/membership filters".into(),
            ));
        }
        if matches!(filter.op.as_str(), "=" | "in" | "not-in")
            && system.property_type(&filter.property) == Some("integer")
        {
            for value in if filter.op == "=" {
                vec![filter.value.as_str()]
            } else {
                filter.value.split(',').collect()
            } {
                value
                    .parse::<i32>()
                    .map_err(|_| Error("Integer filters require a 32-bit integer".into()))?;
            }
        }
        if filter.op == "regex"
            && (boolean
                || system
                    .property_type(&filter.property)
                    .is_some_and(|kind| !matches!(kind, "code" | "string")))
        {
            return Err(Error(
                "Regular expressions support code/string properties only".into(),
            ));
        }
        let expression = if filter.op == "regex" {
            if filter.value.len() > 1024 || work.regex_count >= 32 {
                return Err(Error(
                    "Regular expression count or source size limit exceeded".into(),
                ));
            }
            work.regex_count += 1;
            let expression = Regex::builder()
                .configure(
                    Regex::config()
                        .nfa_size_limit(Some(64 * 1024))
                        .dfa(false)
                        .hybrid(false)
                        .onepass(false)
                        .backtrack(false),
                )
                .syntax(syntax::Config::new().nest_limit(32).case_insensitive(
                    !system.case_sensitive
                        && matches!(filter.property.as_str(), "code" | "concept"),
                ))
                .build(&format!("\\A(?:{})\\z", filter.value))
                .map_err(|error| {
                    Error(format!(
                        "Invalid or unsupported bounded regular expression: {error}"
                    ))
                })?;
            work.regex_bytes += expression.memory_usage();
            if work.regex_bytes > 1024 * 1024 {
                return Err(Error(
                    "Regular expression compiled memory budget exceeded".into(),
                ));
            }
            Some(expression)
        } else {
            None
        };
        let ancestors = if filter.op == "generalizes" {
            let ancestors = system
                .ancestor_depths_work(
                    anchor.as_deref().expect("Selected hierarchy anchor"),
                    &mut work.hierarchy_work,
                )?
                .into_keys()
                .collect::<std::collections::BTreeSet<_>>();
            work.ancestor_bytes += ancestors
                .iter()
                .map(|code| 256 + 4 * code.len())
                .sum::<usize>();
            if work.ancestor_bytes > 8 * 1024 * 1024 {
                return Err(Error(
                    "Finite hierarchy filter memory budget exceeded".into(),
                ));
            }
            Some(ancestors)
        } else {
            None
        };
        let codes = if matches!(filter.property.as_str(), "code" | "concept")
            && matches!(filter.op.as_str(), "=" | "in" | "not-in")
        {
            let values = if filter.op == "=" {
                vec![filter.value.as_str()]
            } else {
                filter.value.split(',').collect()
            };
            Some(
                values
                    .into_iter()
                    .filter_map(|value| system.canonical_code(value).map(str::to_owned))
                    .collect(),
            )
        } else {
            None
        };
        Ok(Self {
            definition: filter,
            expression,
            ancestors,
            anchor,
            codes,
        })
    }
    pub(super) fn matches(
        &self,
        system: &PreparedSystem,
        concept: &Concept,
        work: &mut Evaluation,
    ) -> Result<bool> {
        let filter = self.definition;
        work.charge(1 + concept.property.len())?;
        if filter.property == "concept"
            && matches!(
                filter.op.as_str(),
                "is-a" | "descendent-of" | "is-not-a" | "child-of" | "generalizes"
            )
        {
            let anchor = self.anchor.as_deref().expect("Selected hierarchy anchor");
            if filter.op == "generalizes" {
                return Ok(concept.code == anchor
                    || self
                        .ancestors
                        .as_ref()
                        .expect("Prepared ancestor filter")
                        .contains(&concept.code));
            }
            if filter.op == "child-of" {
                return Ok(system
                    .parents
                    .get(&concept.code)
                    .is_some_and(|parents| parents.contains(anchor)));
            }
            let descendant = system
                .ancestor_depths_work(&concept.code, &mut work.hierarchy_work)?
                .contains_key(anchor);
            return Ok(match filter.op.as_str() {
                "is-a" => concept.code == anchor || descendant,
                "descendent-of" => descendant,
                _ => concept.code != anchor && !descendant,
            });
        }
        let code = matches!(filter.property.as_str(), "code" | "concept");
        let values = concept
            .property
            .iter()
            .filter(|property| property.code == filter.property)
            .collect::<Vec<_>>();
        let default_false = system.r4_semantics
            && values.is_empty()
            && matches!(
                system.property_meaning(&filter.property),
                Some("inactive" | "notSelectable" | "abstract")
            );
        match filter.op.as_str() {
            "=" if default_false => Ok(filter.value == "false"),
            "=" if code => Ok(self
                .codes
                .as_ref()
                .expect("Prepared code filter")
                .contains(&concept.code)),
            "=" => {
                let mut matched = false;
                for property in values {
                    matched |= equality_matches(property, &filter.value)?;
                }
                Ok(matched)
            }
            "exists" => Ok(if code {
                filter.value == "true"
            } else {
                !values.is_empty() == (filter.value == "true")
            }),
            "in" | "not-in" if code => {
                let matched = self
                    .codes
                    .as_ref()
                    .expect("Prepared code filter")
                    .contains(&concept.code);
                Ok(if filter.op == "not-in" {
                    !matched
                } else {
                    matched
                })
            }
            "in" | "not-in" => {
                let mut matched = false;
                for expected in filter.value.split(',') {
                    if default_false {
                        matched |= expected == "false";
                    } else {
                        for property in &values {
                            matched |= equality_matches(property, expected)?;
                        }
                    }
                }
                Ok(if filter.op == "not-in" {
                    !matched
                } else {
                    matched
                })
            }
            "regex" => {
                let expression = self.expression.as_ref().expect("Prepared expression");
                let mut matches = false;
                if code {
                    matches = self.regex_match(expression, &concept.code, work)?;
                } else {
                    for property in values {
                        let (kind, value) = property.typed_value()?;
                        if !matches!(kind, "valueCode" | "valueString") {
                            return Err(Error(
                                "Regular expressions support code/string properties only".into(),
                            ));
                        }
                        let value = value
                            .as_str()
                            .ok_or_else(|| Error("Invalid string property".into()))?;
                        matches |= self.regex_match(expression, value, work)?;
                    }
                }
                Ok(matches)
            }
            _ => Err(Error("Unsupported filter".into())),
        }
    }
    fn regex_match(&self, expression: &Regex, value: &str, work: &mut Evaluation) -> Result<bool> {
        if value.len() > 4096 {
            return Err(Error(
                "Regular expression scalar input exceeds 4096 bytes".into(),
            ));
        }
        work.charge(
            self.definition
                .value
                .len()
                .max(16)
                .saturating_mul(value.len().max(1)),
        )?;
        Ok(expression.is_match(value))
    }
}
