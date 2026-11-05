//! Immutable grammar-backed UCUM provider; not an enumeration of common units.
use crate::{Error, Result};
use octofhir_ucum::{evaluate, parse_expression, ErrorKind};

pub const SYSTEM: &str = "http://unitsofmeasure.org";
pub const VERSION: &str = octofhir_ucum::UCUM_VERSION;
pub const REVISION_DATE: &str = octofhir_ucum::UCUM_REVISION_DATE;
pub const MAX_EXPRESSION_BYTES: usize = 1024;
pub const MAX_EXPLICIT_CODES: usize = 1024;

#[derive(Debug)]
pub struct UcumConcept {
    pub code: String,
    pub display: String,
}

pub struct UcumProvider;

impl UcumProvider {
    /// Prepare a bounded explicit selection, never an enumeration of the UCUM grammar.
    /// The caller supplies a fragment for bundled 2.2; displays come from the real provider.
    /// Supplemental properties retain the normal strict R4 preparation rules.
    pub fn prepare_explicit(mut system: crate::CodeSystem) -> Result<crate::PreparedSystem> {
        if system.url != SYSTEM
            || system.version.as_deref() != Some(VERSION)
            || system.content != "fragment"
            || system.case_sensitive == Some(false)
            || system.hierarchy_meaning.is_some()
            || system.concept.len() > MAX_EXPLICIT_CODES
        {
            return Err(Error("Explicit UCUM preparation requires a case-sensitive bundled 2.2 fragment with at most 1024 flat codes".into()));
        }
        for concept in &mut system.concept {
            if !concept.concept.is_empty() {
                return Err(Error(
                    "UCUM explicit projections cannot contain a hierarchy".into(),
                ));
            }
            let selected = Self::lookup(&concept.code, Some(VERSION))?
                .ok_or_else(|| Error(format!("Invalid explicit UCUM code {}", concept.code)))?;
            concept.display = Some(selected.display);
        }
        // Strictly validate all finite metadata, then retain partial grammar knowledge.
        // No caller can use this prepared subset as a whole-system expansion.
        system.content = "complete".into();
        let mut prepared = crate::PreparedSystem::prepare_for_r4(system)?;
        prepared.content_mode = "fragment".into();
        prepared.explicit_projection = true;
        Ok(prepared)
    }

    /// None means an invalid code, not an unavailable version or exceeded engine limit.
    pub fn lookup(code: &str, version: Option<&str>) -> Result<Option<UcumConcept>> {
        if version.is_some_and(|v| v != VERSION) {
            return Err(Error(format!(
                "UCUM version is unavailable; bundled version is {VERSION}"
            )));
        }
        check_size(code)?;
        // FHIR code primitives cannot be empty, even though UCUM's empty term denotes unity.
        if code.is_empty() {
            return Ok(None);
        }
        let evaluated = match parse_expression(code) {
            Ok(expression) => evaluate(&expression),
            Err(error) => Err(error),
        };
        match evaluated {
            Ok(_) => Ok(Some(UcumConcept {
                code: code.to_owned(),
                display: octofhir_ucum::get_common_display(code),
            })),
            Err(error)
                if matches!(
                    error.kind,
                    ErrorKind::InvalidExpression { .. }
                        | ErrorKind::UnitNotFound { .. }
                        | ErrorKind::SpecialUnitError { .. }
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(Error(format!(
                "UCUM evaluation failed ({}): {error}",
                error.kind.code()
            ))),
        }
    }

    /// Invalid operands are errors, not evidence that two valid units are incomparable.
    pub fn comparable(left: &str, right: &str) -> Result<bool> {
        for code in [left, right] {
            check_size(code)?;
            if code.is_empty() {
                return Err(Error("A UCUM code must not be empty".into()));
            }
        }
        octofhir_ucum::is_comparable(left, right).map_err(|e| Error(e.to_string()))
    }
}

fn check_size(code: &str) -> Result<()> {
    if code.len() > MAX_EXPRESSION_BYTES {
        return Err(Error("UCUM expression exceeds 1024 bytes".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_covers_prefixed_compound_annotated_and_special_codes() {
        assert_eq!(VERSION, "2.2");
        for code in [
            "m",
            "mg",
            "mg/dL",
            "kg.m/s2",
            "mm[Hg]",
            "Cel",
            "[degF]",
            "mL/min",
            "{count}/min",
            "1",
        ] {
            let concept = UcumProvider::lookup(code, Some(VERSION)).unwrap().unwrap();
            assert_eq!(concept.code, code);
            assert!(!concept.display.is_empty());
        }
        assert_eq!(
            UcumProvider::lookup("mg", None).unwrap().unwrap().display,
            "milligram"
        );
        assert!(UcumProvider::lookup("mg/dL", None)
            .unwrap()
            .unwrap()
            .display
            .contains("milligram"));
    }

    #[test]
    fn invalid_units_and_case_are_not_successful_lookups() {
        for code in ["", "not-a-unit", "PAL", "mg//dL", "mg/", "unknown_unit"] {
            assert!(
                UcumProvider::lookup(code, None).unwrap().is_none(),
                "{code}"
            );
        }
        assert!(UcumProvider::lookup("Pa", None).unwrap().is_some());
    }

    #[test]
    fn resource_limits_and_unavailable_versions_are_not_invalid_code_answers() {
        assert!(UcumProvider::lookup("mg", Some("unknown-version")).is_err());
        assert!(UcumProvider::lookup(&"m".repeat(MAX_EXPRESSION_BYTES + 1), None).is_err());
        let depth = octofhir_ucum::MAX_NESTING_DEPTH + 1;
        let nested = format!("{}m{}", "(".repeat(depth), ")".repeat(depth));
        assert!(UcumProvider::lookup(&nested, None).is_err());
    }

    #[test]
    fn comparability_uses_the_library_not_string_or_display_equality() {
        for (left, right, expected) in [
            ("mg", "g", true),
            ("mg/dL", "g/L", true),
            ("Cel", "[degF]", true),
            ("m", "s", false),
            ("m2", "m", false),
        ] {
            assert_eq!(UcumProvider::comparable(left, right).unwrap(), expected);
        }
        assert!(UcumProvider::comparable("m", "not-a-unit").is_err());
        assert!(UcumProvider::comparable("", "m").is_err());
        assert!(UcumProvider::comparable(&"m".repeat(MAX_EXPRESSION_BYTES + 1), "m").is_err());
    }
}

#[cfg(test)]
mod explicit_tests {
    use super::*;
    use crate::{Canonical, Compose, Snapshot};
    use serde_json::json;

    fn source() -> crate::CodeSystem {
        serde_json::from_value(json!({"url":SYSTEM,"version":VERSION,"content":"fragment","concept":[{"code":"mg","display":"Not the provider label"},{"code":"mg/dL"},{"code":"Cel"}]})).unwrap()
    }
    #[test]
    fn explicit_grammar_projection_cannot_become_a_whole_system_or_complete_code_index() {
        let prepared = UcumProvider::prepare_explicit(source()).unwrap();
        assert_eq!(
            prepared.concept("mg").unwrap().display.as_deref(),
            Some("milligram")
        );
        let key = Canonical::parse(SYSTEM, None).unwrap();
        let root = Canonical::parse("urn:explicit-units", None).unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.systems.insert(key, prepared);
        let compose = |value| serde_json::from_value::<Compose>(value).unwrap();
        snapshot.value_sets.insert(root.clone(),compose(json!({"include":[{"system":SYSTEM,"concept":[{"code":"mg"},{"code":"Cel"}]}],"exclude":[{"system":SYSTEM,"concept":[{"code":"Cel"}]}]})));
        let members = snapshot.expand(&root).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members.values().next().unwrap().code, "mg");
        for include in [
            json!({"system":SYSTEM}),
            json!({"system":SYSTEM,"concept":[{"code":"mg"}],"filter":[{"property":"code","op":"=","value":"mg"}]}),
            json!({"system":SYSTEM,"concept":[{"code":"kg"}]}),
        ] {
            snapshot
                .value_sets
                .insert(root.clone(), compose(json!({"include":[include]})));
            assert!(snapshot.expand(&root).is_err());
        }
        let index = snapshot
            .systems
            .into_values()
            .next()
            .unwrap()
            .into_code_index();
        assert!(!index.complete());
        assert_eq!(index.resolve("mg"), Some("mg"));
        assert_eq!(index.resolve("kg"), None);
    }
    #[test]
    fn explicit_grammar_projection_validates_identity_codes_types_and_limits() {
        for change in [
            "version",
            "system",
            "case",
            "mode",
            "hierarchy",
            "code",
            "nested",
            "count",
        ] {
            let mut system = source();
            match change {
                "version" => system.version = Some("unavailable".into()),
                "system" => system.url = "urn:other".into(),
                "case" => system.case_sensitive = Some(false),
                "mode" => system.content = "complete".into(),
                "hierarchy" => system.hierarchy_meaning = Some("is-a".into()),
                "code" => system.concept[0].code = "not-a-unit".into(),
                "nested" => system.concept[0].concept = source().concept,
                "count" => {
                    system.concept = (0..=MAX_EXPLICIT_CODES)
                        .map(|i| {
                            serde_json::from_value(json!({"code":format!("1{{fixture{i}}}")}))
                                .unwrap()
                        })
                        .collect();
                }
                _ => unreachable!(),
            }
            assert!(UcumProvider::prepare_explicit(system).is_err(), "{change}");
        }
    }
}
