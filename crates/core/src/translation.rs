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
    url: Option<Arc<str>>,
    version: Option<Arc<str>>,
    source_scope: Option<Arc<str>>,
    target_scope: Option<Arc<str>>,
    charge: usize,
}
struct Group {
    source_version: Option<Arc<str>>,
    target_version: Option<Arc<str>>,
    source: Option<Arc<str>>,
    target: Option<Arc<str>>,
    mappings: Vec<Mapping>,
    source_codes: BTreeSet<Arc<str>>,
    target_codes: BTreeSet<Arc<str>>,
    forward: BTreeMap<Arc<str>, Vec<usize>>,
    reverse: BTreeMap<Arc<str>, Vec<usize>>,
    conditional_codes: BTreeSet<Arc<str>>,
    reverse_conditional: bool,
    unmapped: bool,
    unmapped_rule: Option<UnmappedRule>,
}
struct Mapping {
    source: Arc<str>,
    source_display: Option<Arc<str>>,
    target: Arc<str>,
    target_display: Option<Arc<str>>,
    equivalence: Arc<str>,
    conditions: Vec<Criterion>,
    products: Vec<Criterion>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct TranslationMatch<'a> {
    pub system: &'a str,
    pub code: &'a str,
    pub display: Option<&'a str>,
    pub equivalence: &'a str,
}
pub struct VersionedTranslationMatch<'a> {
    pub concept: TranslationMatch<'a>,
    pub version: Option<&'a str>,
}
struct UnmappedRule {
    mode: Option<Arc<str>>,
    code: Option<Arc<str>>,
    display: Option<Arc<str>>,
    url: Option<Arc<str>>,
}
struct Criterion {
    property: Option<Arc<str>>,
    system: Option<Arc<str>>,
    code: Option<Arc<str>>,
    display: Option<Arc<str>>,
}
pub struct TranslationDependency<'a> {
    pub element: &'a str,
    pub system: Option<&'a str>,
    pub version: Option<&'a str>,
    pub code: &'a str,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TranslationProduct {
    pub element: String,
    pub system: Option<String>,
    pub code: String,
    pub display: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResolvedTranslation {
    pub system: String,
    pub version: Option<String>,
    pub code: String,
    pub display: Option<String>,
    pub equivalence: String,
    pub products: Vec<TranslationProduct>,
}
pub enum TranslationContinuation {
    OtherMap {
        canonical: crate::Canonical,
        group: usize,
    },
    FixedReverse {
        system: String,
        version: Option<String>,
        group: usize,
    },
}
pub struct TranslationResolution {
    pub matches: Vec<ResolvedTranslation>,
    pub continuations: Vec<TranslationContinuation>,
}
impl PreparedConceptMap {
    pub fn prepare(content: &Value) -> Result<Self> {
        let mut plan = Self::default();
        plan.url = text(content, "url", &mut plan.charge);
        plan.version = text(content, "version", &mut plan.charge);
        plan.source_scope = text(content, "sourceCanonical", &mut plan.charge)
            .or_else(|| text(content, "sourceUri", &mut plan.charge));
        plan.target_scope = text(content, "targetCanonical", &mut plan.charge)
            .or_else(|| text(content, "targetUri", &mut plan.charge));
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
                source_version: text(group, "sourceVersion", &mut plan.charge),
                target_version: text(group, "targetVersion", &mut plan.charge),
                source: text(group, "source", &mut plan.charge),
                target: text(group, "target", &mut plan.charge),
                mappings: Vec::new(),
                source_codes: BTreeSet::new(),
                target_codes: BTreeSet::new(),
                forward: BTreeMap::new(),
                reverse: BTreeMap::new(),
                conditional_codes: BTreeSet::new(),
                reverse_conditional: false,
                unmapped: group.get("unmapped").is_some(),
                unmapped_rule: group.get("unmapped").map(|v| UnmappedRule {
                    mode: text(v, "mode", &mut plan.charge),
                    code: text(v, "code", &mut plan.charge),
                    display: text(v, "display", &mut plan.charge),
                    url: text(v, "url", &mut plan.charge),
                }),
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
                if let Some(code) = &source {
                    prepared.source_codes.insert(code.clone());
                }
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
                    }
                    let conditions = criteria(target, "dependsOn", &mut plan.charge, &mut edges)?;
                    let products = criteria(target, "product", &mut plan.charge, &mut edges)?;
                    if target.get("relationship").is_some() {
                        return Err(Error(
                            "R5 ConceptMap relationship is not an R4 equivalence".into(),
                        ));
                    }
                    let equivalence = target
                        .get("equivalence")
                        .and_then(Value::as_str)
                        .unwrap_or("equivalent");
                    if !matches!(
                        equivalence,
                        "relatedto"
                            | "equivalent"
                            | "equal"
                            | "wider"
                            | "subsumes"
                            | "narrower"
                            | "specializes"
                            | "inexact"
                            | "unmatched"
                            | "disjoint"
                    ) {
                        return Err(Error("Invalid R4 mapping equivalence".into()));
                    }
                    if let Some(code) = text(target, "code", &mut plan.charge) {
                        prepared.target_codes.insert(code);
                    }
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
                        conditions,
                        products,
                    });
                }
            }
            plan.groups.push(prepared);
        }
        Ok(plan)
    }
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    pub fn input_system(&self, reverse: bool) -> Option<&str> {
        let mut systems = self.groups.iter().filter_map(|g| {
            if reverse {
                g.target.as_deref()
            } else {
                g.source.as_deref()
            }
        });
        let first = systems.next()?;
        systems.all(|s| s == first).then_some(first)
    }
    pub fn source_scope(&self, reverse: bool) -> Option<&str> {
        if reverse {
            self.target_scope.as_deref()
        } else {
            self.source_scope.as_deref()
        }
    }
    pub fn target_scope(&self, reverse: bool) -> Option<&str> {
        if reverse {
            self.source_scope.as_deref()
        } else {
            self.target_scope.as_deref()
        }
    }
    /// Store identity and payload must originate in the same reader snapshot.
    pub fn prepare_with_identity(
        content: &Value,
        url: &str,
        version: Option<&str>,
    ) -> Result<Self> {
        let mut plan = Self::prepare(content)?;
        plan.charge += 128 + url.len() * 8 + version.map_or(0, |v| 128 + v.len() * 8);
        plan.url = Some(Arc::from(url));
        plan.version = version.map(Arc::from);
        Ok(plan)
    }
    pub fn translate<'a>(
        &'a self,
        system: &str,
        code: &str,
        target_system: Option<&str>,
        reverse: bool,
    ) -> Result<Vec<TranslationMatch<'a>>> {
        self.translate_impl(system, None, code, target_system, reverse, |concept, _| {
            concept
        })
    }
    pub fn translate_versioned<'a>(
        &'a self,
        system: &str,
        version: Option<&str>,
        code: &str,
        target_system: Option<&str>,
        reverse: bool,
    ) -> Result<Vec<VersionedTranslationMatch<'a>>> {
        self.translate_impl(
            system,
            version,
            code,
            target_system,
            reverse,
            |concept, version| VersionedTranslationMatch { concept, version },
        )
    }
    fn translate_impl<'a, T>(
        &'a self,
        system: &str,
        version: Option<&str>,
        code: &str,
        target_system: Option<&str>,
        reverse: bool,
        convert: impl Fn(TranslationMatch<'a>, Option<&'a str>) -> T,
    ) -> Result<Vec<T>> {
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
            let input_version = if reverse {
                group.target_version.as_deref()
            } else {
                group.source_version.as_deref()
            };
            if input_version.zip(version).is_some_and(|(a, b)| a != b) {
                continue;
            }
            let output_version = if reverse {
                group.source_version.as_deref()
            } else {
                group.target_version.as_deref()
            };
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
                matches.push(convert(
                    TranslationMatch {
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
                    },
                    output_version,
                ));
            }
        }
        Ok(matches)
    }
    pub fn fallback_accepts(
        &self,
        group: usize,
        system: &str,
        version: Option<&str>,
        code: &str,
    ) -> bool {
        self.groups.get(group).is_some_and(|g| {
            g.source.as_deref() == Some(system)
                && !g
                    .source_version
                    .as_deref()
                    .zip(version)
                    .is_some_and(|(a, b)| a != b)
                && !g.source_codes.contains(code)
        })
    }
    pub fn explicit_source_codes(&self, group: usize) -> Vec<&str> {
        self.groups
            .get(group)
            .map(|g| g.source_codes.iter().map(|k| k.as_ref()).collect())
            .unwrap_or_default()
    }
    pub fn translate_resolved(
        &self,
        system: &str,
        version: Option<&str>,
        code: &str,
        target_system: Option<&str>,
        reverse: bool,
        dependencies: &[TranslationDependency<'_>],
    ) -> Result<TranslationResolution> {
        let mut result = TranslationResolution {
            matches: Vec::new(),
            continuations: Vec::new(),
        };
        for (group_index, group) in self.groups.iter().enumerate() {
            let (input_system, input_version, output_system, output_version) = if reverse {
                (
                    &group.target,
                    &group.target_version,
                    &group.source,
                    &group.source_version,
                )
            } else {
                (
                    &group.source,
                    &group.source_version,
                    &group.target,
                    &group.target_version,
                )
            };
            if input_system.as_deref() != Some(system)
                || input_version
                    .as_deref()
                    .zip(version)
                    .is_some_and(|(a, b)| a != b)
            {
                continue;
            }
            let Some(output_system) = output_system.as_deref() else {
                if group.mappings.is_empty() && group.unmapped_rule.is_none() {
                    continue;
                }
                return Err(Error("ConceptMap output system is unknown".into()));
            };
            if target_system.is_some_and(|target| target != output_system) {
                continue;
            }
            let index = if reverse {
                &group.reverse
            } else {
                &group.forward
            };
            for entry in index.get(code).into_iter().flatten() {
                let mapping = &group.mappings[*entry];
                let (conditions, products) = if reverse {
                    (&mapping.products, &mapping.conditions)
                } else {
                    (&mapping.conditions, &mapping.products)
                };
                let mut applies = true;
                for condition in conditions {
                    let property = condition
                        .property
                        .as_deref()
                        .ok_or_else(|| Error("Mapping dependency property is required".into()))?;
                    let value = condition
                        .code
                        .as_deref()
                        .ok_or_else(|| Error("Mapping dependency value is required".into()))?;
                    let choices: Vec<_> = dependencies
                        .iter()
                        .filter(|d| d.element == property)
                        .collect();
                    if choices.is_empty() {
                        return Err(Error(format!("Mapping requires dependency {property}")));
                    }
                    let context = condition
                        .system
                        .as_deref()
                        .map(|s| crate::Canonical::parse(s, None))
                        .transpose()?;
                    if !choices.iter().any(|d| {
                        d.code == value
                            && context.as_ref().is_none_or(|c| {
                                d.system == Some(c.url.as_str())
                                    && c.version.as_deref().is_none_or(|v| d.version == Some(v))
                            })
                    }) {
                        applies = false;
                        break;
                    }
                }
                if !applies {
                    continue;
                }
                let products = products
                    .iter()
                    .map(|p| {
                        Ok(TranslationProduct {
                            element: p
                                .property
                                .as_ref()
                                .ok_or_else(|| {
                                    Error("Mapping product property is required".into())
                                })?
                                .to_string(),
                            system: p.system.as_ref().map(ToString::to_string),
                            code: p
                                .code
                                .as_ref()
                                .ok_or_else(|| Error("Mapping product value is required".into()))?
                                .to_string(),
                            display: p.display.as_ref().map(ToString::to_string),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                result.matches.push(ResolvedTranslation {
                    system: output_system.into(),
                    version: output_version.as_ref().map(ToString::to_string),
                    code: if reverse {
                        mapping.source.to_string()
                    } else {
                        mapping.target.to_string()
                    },
                    display: if reverse {
                        mapping.source_display.as_ref().map(ToString::to_string)
                    } else {
                        mapping.target_display.as_ref().map(ToString::to_string)
                    },
                    equivalence: if reverse {
                        reverse_equivalence(&mapping.equivalence).into()
                    } else {
                        mapping.equivalence.to_string()
                    },
                    products,
                });
            }
            if !reverse && group.source_codes.contains(code) {
                continue;
            }
            if let Some(rule) = &group.unmapped_rule {
                match rule.mode.as_deref() {
                    Some("provided") if !reverse || !group.source_codes.contains(code) => {
                        result.matches.push(ResolvedTranslation {
                            system: output_system.into(),
                            version: output_version.as_ref().map(ToString::to_string),
                            code: code.into(),
                            display: None,
                            equivalence: "relatedto".into(),
                            products: Vec::new(),
                        })
                    }
                    Some("provided") => {}
                    Some("fixed") if !reverse => result.matches.push(ResolvedTranslation {
                        system: output_system.into(),
                        version: output_version.as_ref().map(ToString::to_string),
                        code: rule
                            .code
                            .as_deref()
                            .ok_or_else(|| Error("Fixed unmapped code is required".into()))?
                            .into(),
                        display: rule.display.as_deref().map(str::to_owned),
                        equivalence: "relatedto".into(),
                        products: Vec::new(),
                    }),
                    Some("fixed") if rule.code.as_deref() == Some(code) => result
                        .continuations
                        .push(TranslationContinuation::FixedReverse {
                            system: output_system.into(),
                            version: output_version.as_ref().map(ToString::to_string),
                            group: group_index,
                        }),
                    Some("fixed") => {}
                    Some("other-map") => {
                        result
                            .continuations
                            .push(TranslationContinuation::OtherMap {
                                canonical: crate::Canonical::parse(
                                    rule.url.as_deref().ok_or_else(|| {
                                        Error("Unmapped ConceptMap URL required".into())
                                    })?,
                                    None,
                                )?,
                                group: group_index,
                            })
                    }
                    _ => return Err(Error("Invalid ConceptMap unmapped mode".into())),
                }
            }
        }
        Ok(result)
    }
    /// Conservative retained-memory charge, including index nodes and shared string buffers.
    pub fn memory_charge(&self) -> usize {
        self.charge
    }
}
fn criteria(
    value: &Value,
    key: &str,
    charge: &mut usize,
    records: &mut usize,
) -> Result<Vec<Criterion>> {
    let Some(values) = value.get(key) else {
        return Ok(Vec::new());
    };
    let values = values
        .as_array()
        .ok_or_else(|| Error("ConceptMap conditions/products must be arrays".into()))?;
    let mut result = Vec::new();
    for value in values {
        *records += 1;
        if *records > MAX_CONCEPTS {
            return Err(Error("ConceptMap detail record limit exceeded".into()));
        }
        *charge += 256;
        result.push(Criterion {
            property: text(value, "property", charge),
            system: text(value, "system", charge),
            code: text(value, "value", charge),
            display: text(value, "display", charge),
        });
    }
    Ok(result)
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
    fn dependencies_are_required_constraints_and_products_reverse_with_the_mapping() {
        let plan=PreparedConceptMap::prepare(&json!({"group":[{"source":"s","target":"t","element":[{"code":"a","target":[{"code":"b","equivalence":"wider","dependsOn":[{"property":"urn:context","system":"urn:kind","value":"in"}],"product":[{"property":"urn:result","system":"urn:kind","value":"out","display":"Output"}]}]}]}]})).unwrap();
        assert!(plan
            .translate_resolved("s", None, "a", None, false, &[])
            .is_err());
        let dependencies = [TranslationDependency {
            element: "urn:context",
            system: Some("urn:kind"),
            version: None,
            code: "wrong",
        }];
        assert!(plan
            .translate_resolved("s", None, "a", None, false, &dependencies)
            .unwrap()
            .matches
            .is_empty());
        let dependencies = [TranslationDependency {
            code: "in",
            ..dependencies[0]
        }];
        let matched = plan
            .translate_resolved("s", None, "a", None, false, &dependencies)
            .unwrap()
            .matches;
        assert_eq!(matched[0].code, "b");
        assert_eq!(matched[0].products[0].code, "out");
        assert_eq!(matched[0].products[0].element, "urn:result");
        let dependencies = [TranslationDependency {
            element: "urn:result",
            system: Some("urn:kind"),
            version: None,
            code: "out",
        }];
        let reversed = plan
            .translate_resolved("t", None, "b", None, true, &dependencies)
            .unwrap()
            .matches;
        assert_eq!(reversed[0].code, "a");
        assert_eq!(reversed[0].equivalence, "narrower");
        assert_eq!(reversed[0].products[0].code, "in");
    }

    #[test]
    fn unmapped_rules_never_override_explicit_negative_or_conditional_entries() {
        let plan=PreparedConceptMap::prepare(&json!({"group":[{"source":"s","target":"t","element":[{"code":"negative","target":[{"code":"no","equivalence":"disjoint"}]},{"code":"conditional","target":[{"code":"yes","dependsOn":[{"property":"urn:p","value":"required"}]}]}],"unmapped":{"mode":"provided"}}]})).unwrap();
        assert!(plan
            .translate_resolved("s", None, "negative", None, false, &[])
            .unwrap()
            .matches
            .is_empty());
        assert!(plan
            .translate_resolved("s", None, "conditional", None, false, &[])
            .is_err());
        let dependency = [TranslationDependency {
            element: "urn:p",
            system: None,
            version: None,
            code: "wrong",
        }];
        assert!(plan
            .translate_resolved("s", None, "conditional", None, false, &dependency)
            .unwrap()
            .matches
            .is_empty());
        let matched = plan
            .translate_resolved("s", None, "unmapped", None, false, &[])
            .unwrap()
            .matches;
        assert_eq!(matched[0].system, "t");
        assert_eq!(matched[0].code, "unmapped");
        assert!(!plan.fallback_accepts(0, "s", None, "negative"));
    }

    #[test]
    fn fixed_and_referenced_unmapped_modes_preserve_explicit_matches_and_continuations() {
        let fixed=PreparedConceptMap::prepare(&json!({"group":[{"source":"s","target":"t","element":[{"code":"explicit","target":[{"code":"fallback"}]}],"unmapped":{"mode":"fixed","code":"fallback","display":"Fallback"}}]})).unwrap();
        assert_eq!(
            fixed
                .translate_resolved("s", None, "unmapped", None, false, &[])
                .unwrap()
                .matches[0]
                .display
                .as_deref(),
            Some("Fallback")
        );
        let reverse = fixed
            .translate_resolved("t", None, "fallback", None, true, &[])
            .unwrap();
        assert_eq!(reverse.matches[0].code, "explicit");
        assert!(matches!(
            reverse.continuations[0],
            TranslationContinuation::FixedReverse { group: 0, .. }
        ));
        let referenced=PreparedConceptMap::prepare(&json!({"group":[{"source":"s","target":"t","unmapped":{"mode":"other-map","url":"urn:other|1"}}]})).unwrap();
        let resolved = referenced
            .translate_resolved("s", None, "a", None, false, &[])
            .unwrap();
        assert!(
            matches!(&resolved.continuations[0],TranslationContinuation::OtherMap {canonical,..} if canonical.url=="urn:other" && canonical.version.as_deref()==Some("1"))
        );
    }
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
