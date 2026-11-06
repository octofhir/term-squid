//! Explicit primary-table scope for native LOINC, without inventing Part relations.
use crate::{Canonical, Error, Filter, Result};

pub const SYSTEM: &str = "http://loinc.org";
pub const VALUE_SET: &str = "http://loinc.org/vs";
pub const SOURCE_PROPERTY: &str = "source-table";
pub const SOURCE_TABLE: &str = "Loinc.csv";

/// The six coded primary axes, independent of their verbatim CSV labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryAxis {
    Component,
    Property,
    Time,
    System,
    Scale,
    Method,
}
impl PrimaryAxis {
    pub const ALL: [Self; 6] = [
        Self::Component,
        Self::Property,
        Self::Time,
        Self::System,
        Self::Scale,
        Self::Method,
    ];
    pub fn all() -> [Self; 6] {
        Self::ALL
    }
    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|axis| axis.code() == code)
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::Component => "COMPONENT",
            Self::Property => "PROPERTY",
            Self::Time => "TIME_ASPCT",
            Self::System => "SYSTEM",
            Self::Scale => "SCALE_TYP",
            Self::Method => "METHOD_TYP",
        }
    }
    pub fn part_type(self) -> &'static str {
        match self {
            Self::Time => "TIME",
            Self::Scale => "SCALE",
            Self::Method => "METHOD",
            _ => self.code(),
        }
    }
    pub fn uri(self) -> &'static str {
        match self {
            Self::Component => "http://loinc.org/property/COMPONENT",
            Self::Property => "http://loinc.org/property/PROPERTY",
            Self::Time => "http://loinc.org/property/TIME_ASPCT",
            Self::System => "http://loinc.org/property/SYSTEM",
            Self::Scale => "http://loinc.org/property/SCALE_TYP",
            Self::Method => "http://loinc.org/property/METHOD_TYP",
        }
    }
}

/// Normalize only the ASCII Part prefix; this does not establish Part existence.
pub fn normalize_part_code(code: &str) -> Result<String> {
    if code.len() <= 255
        && code
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("LP"))
        && code.get(2..).is_some_and(|suffix| {
            suffix.split_once('-').is_some_and(|(number, check)| {
                !number.is_empty()
                    && number.bytes().all(|b| b.is_ascii_digit())
                    && check.len() == 1
                    && check.as_bytes()[0].is_ascii_digit()
            })
        })
    {
        Ok(format!("LP{}", &code[2..]))
    } else {
        Err(Error(
            "LOINC primary axis filters require an LP Part identifier".into(),
        ))
    }
}

/// Accessory code families need their own source files before absence is conclusive.
pub fn require_primary_code(code: &str) -> Result<()> {
    if code.len() <= 255
        && code.split_once('-').is_some_and(|(number, check)| {
            !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit())
                && check.len() == 1
                && check.as_bytes()[0].is_ascii_digit()
        })
    {
        Ok(())
    } else {
        Err(Error(
            "Native LOINC supports primary numeric terms only; accessory codes require additional source files"
                .into(),
        ))
    }
}

/// CSV axis labels use distinct property names; they are not LP Part identifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrimaryFilter {
    Code(String),
    Parts {
        axis: PrimaryAxis,
        values: Vec<String>,
    },
    PartExists {
        axis: PrimaryAxis,
        present: bool,
    },
    Equality {
        property: String,
        value: String,
    },
    Exists {
        property: String,
        present: bool,
    },
}
impl PrimaryFilter {
    pub fn parse(filter: &Filter) -> Result<Self> {
        let Filter {
            property,
            op,
            value,
        } = filter;
        if matches!(property.as_str(), "concept" | "code") && op == "=" {
            require_primary_code(value)?;
            return Ok(Self::Code(value.clone()));
        }
        if let Some(axis) = PrimaryAxis::parse(property) {
            return match op.as_str() {
                "=" => Ok(Self::Parts {
                    axis,
                    values: vec![normalize_part_code(value)?],
                }),
                "in" => {
                    let values = value
                        .split(',')
                        .take(257)
                        .map(normalize_part_code)
                        .collect::<Result<Vec<_>>>()?;
                    if values.len() > 256 {
                        return Err(Error("LOINC Part filter exceeds 256 values".into()));
                    }
                    Ok(Self::Parts { axis, values })
                }
                "exists" if matches!(value.as_str(), "true" | "false") => Ok(Self::PartExists {
                    axis,
                    present: value == "true",
                }),
                _ => Err(Error(format!(
                    "Unsupported coded LOINC filter {property} {op}"
                ))),
            };
        }
        if !matches!(
            property.as_str(),
            "source-table"
                | "raw-COMPONENT"
                | "raw-PROPERTY"
                | "raw-TIME_ASPCT"
                | "raw-SYSTEM"
                | "raw-SCALE_TYP"
                | "raw-METHOD_TYP"
                | "raw-EXTERNAL_COPYRIGHT_NOTICE"
                | "STATUS"
                | "CLASS"
                | "CLASSTYPE"
                | "ORDER_OBS"
                | "CONSUMER_NAME"
                | "inactive"
        ) {
            return Err(Error(format!(
                "Unsupported primary LOINC property {property}"
            )));
        }
        match op.as_str() {
            "=" => {
                if property == "inactive" && !matches!(value.as_str(), "true" | "false") {
                    return Err(Error("inactive expects true or false".into()));
                }
                Ok(Self::Equality {
                    property: property.clone(),
                    value: value.clone(),
                })
            }
            "exists" if matches!(value.as_str(), "true" | "false") => Ok(Self::Exists {
                property: property.clone(),
                present: value == "true",
            }),
            _ => Err(Error(format!(
                "Unsupported primary LOINC filter {property} {op}"
            ))),
        }
    }
    pub fn scopes_primary_table(&self) -> bool {
        matches!(self, Self::Equality { property, value } if property == SOURCE_PROPERTY && value == SOURCE_TABLE)
            || matches!(self, Self::Code(_))
    }
}

/// The all-code implicit set can validate primary terms, but cannot be enumerated
/// from Loinc.csv alone because it also covers accessory code families.
pub fn implicit(canonical: &Canonical) -> Result<Option<serde_json::Value>> {
    if canonical.url == VALUE_SET {
        let mut include = serde_json::json!({"system":SYSTEM});
        if let Some(version) = &canonical.version {
            include["version"] = version.clone().into();
        }
        let mut resource = serde_json::json!({"resourceType":"ValueSet","url":VALUE_SET,
            "status":"active","compose":{"include":[include]}});
        if let Some(version) = &canonical.version {
            resource["version"] = version.clone().into();
        }
        Ok(Some(resource))
    } else if canonical.url.starts_with(&format!("{VALUE_SET}/")) {
        Err(Error(
            "LOINC accessory/implicit subset resolution requires additional source files".into(),
        ))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_terms_are_distinct_from_unloaded_accessories() {
        for code in ["1000-1", "1234567-8", "9999999-9"] {
            require_primary_code(code).unwrap();
        }
        for code in [
            "LA1-0", "LP1-2", "LL1-0", "LG1-0", "123", "1-12", "1-", "-1", "1-1-1",
        ] {
            assert!(require_primary_code(code).is_err(), "{code}");
        }
    }
    #[test]
    fn filters_preserve_raw_labels_types_and_explicit_scope() {
        let filter = |property: &str, op: &str, value: &str| {
            PrimaryFilter::parse(&Filter {
                property: property.into(),
                op: op.into(),
                value: value.into(),
            })
        };
        assert!(filter("source-table", "=", "Loinc.csv")
            .unwrap()
            .scopes_primary_table());
        assert!(filter("concept", "=", "1000-1")
            .unwrap()
            .scopes_primary_table());
        assert!(!filter("STATUS", "=", "ACTIVE")
            .unwrap()
            .scopes_primary_table());
        assert!(!filter("source-table", "exists", "true")
            .unwrap()
            .scopes_primary_table());
        assert!(filter("raw-METHOD_TYP", "exists", "false").is_ok());
        for (property, op, value) in [
            ("COMPONENT", "=", "Generated component"),
            ("concept", "is-a", "1000-1"),
            ("CLASS", "regex", ".*"),
            ("inactive", "=", "1"),
            ("CLASS", "exists", "yes"),
        ] {
            assert!(filter(property, op, value).is_err());
        }
    }
    #[test]
    fn coded_primary_axes_are_bounded_literals_not_part_inventory() {
        let filter = |property: &str, op: &str, value: &str| {
            PrimaryFilter::parse(&Filter {
                property: property.into(),
                op: op.into(),
                value: value.into(),
            })
        };
        for axis in PrimaryAxis::all() {
            assert_eq!(PrimaryAxis::parse(axis.code()), Some(axis));
            assert_eq!(
                axis.uri(),
                format!("http://loinc.org/property/{}", axis.code())
            );
            let parsed = filter(axis.code(), "in", "lp123-4,LP999999-9").unwrap();
            assert_eq!(
                parsed,
                PrimaryFilter::Parts {
                    axis,
                    values: vec!["LP123-4".into(), "LP999999-9".into()],
                }
            );
            assert!(!parsed.scopes_primary_table());
            assert!(filter(axis.code(), "exists", "false").is_ok());
        }
        assert_eq!(PrimaryAxis::Time.part_type(), "TIME");
        assert_eq!(PrimaryAxis::Scale.part_type(), "SCALE");
        assert_eq!(PrimaryAxis::Method.part_type(), "METHOD");
        for value in [
            "LP", "LP1", "LP1-12", "LP-1", "LP1-1-1", "LA1-0", " LP1-0", "LP1-0 ", "ℓp1-0",
            "LP1-0\n",
        ] {
            assert!(normalize_part_code(value).is_err(), "{value}");
        }
        for value in ["", "LP1-0,", ",LP1-0", "LP1-0,,LP2-0"] {
            assert!(filter("COMPONENT", "in", value).is_err());
        }
        assert!(filter("METHOD_TYP", "exists", "yes").is_err());
        assert!(filter("METHOD_TYP", "regex", "LP.*").is_err());
        assert!(filter("COMPONENT", "in", &vec!["LP1-0"; 256].join(",")).is_ok());
        assert!(filter("COMPONENT", "in", &vec!["LP1-0"; 257].join(",")).is_err());
    }
    #[test]
    fn implicit_whole_set_preserves_version_and_rejects_accessory_subsets() {
        let canonical = Canonical::parse(VALUE_SET, Some("2.82")).unwrap();
        let resource = implicit(&canonical).unwrap().unwrap();
        assert_eq!(resource["version"], "2.82");
        assert_eq!(resource["compose"]["include"][0]["version"], "2.82");
        assert!(implicit(&Canonical::parse("http://loinc.org/vs/LL1-0", None).unwrap()).is_err());
        assert!(implicit(&Canonical::parse("urn:other", None).unwrap())
            .unwrap()
            .is_none());
    }
}
