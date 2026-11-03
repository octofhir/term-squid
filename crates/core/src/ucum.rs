//! Immutable grammar-backed UCUM provider; not an enumeration of common units.
use crate::{Error, Result};
use octofhir_ucum::{evaluate, parse_expression, ErrorKind};

pub const SYSTEM: &str = "http://unitsofmeasure.org";
pub const VERSION: &str = octofhir_ucum::UCUM_VERSION;
pub const MAX_EXPRESSION_BYTES: usize = 1024;

#[derive(Debug)]
pub struct UcumConcept {
    pub code: String,
    pub display: String,
}

pub struct UcumProvider;

impl UcumProvider {
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
