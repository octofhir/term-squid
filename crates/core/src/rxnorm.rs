//! Explicit current-active RxNorm scope; source atoms are not a historical registry.
use crate::{Canonical, Error, Result};

pub const SYSTEM: &str = "http://www.nlm.nih.gov/research/umls/rxnorm";
pub const VALUE_SET: &str = "http://www.nlm.nih.gov/research/umls/rxnorm/vs";
pub const SOURCE_TABLE: &str = "RXNCONSO.RRF";
pub const RELATION_TYPES: &[&str] = &["SY", "SIB", "RN", "PAR", "CHD", "RB", "RO"];
pub const RELATION_LABELS: &[&str] = &[
    "boss_of",
    "consists_of",
    "constitutes",
    "contained_in",
    "contains",
    "dose_form_of",
    "doseformgroup_of",
    "form_of",
    "has_boss",
    "has_dose_form",
    "has_doseformgroup",
    "has_form",
    "has_ingredient",
    "has_ingredients",
    "has_part",
    "has_precise_ingredient",
    "has_quantified_form",
    "has_tradename",
    "ingredient_of",
    "ingredients_of",
    "inverse_isa",
    "isa",
    "part_of",
    "precise_ingredient_of",
    "quantified_form_of",
    "reformulated_to",
    "reformulation_of",
    "tradename_of",
];
pub const PRIMARY_TYPES: &[&str] = &[
    "IN", "PIN", "MIN", "SCDC", "SCDF", "SCDFP", "SCDG", "SCDGP", "SCD", "GPCK", "BN", "SBDC",
    "SBDF", "SBDFP", "SBDG", "SBD", "BPCK", "DF", "DFG", "ET",
];

pub fn require_code(code: &str) -> Result<()> {
    if !code.is_empty()
        && code.len() <= 18
        && !code.starts_with('0')
        && code.bytes().all(|byte| byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(Error(
            "RxNorm requires a positive scalar RXCUI of at most 18 digits".into(),
        ))
    }
}

/// The official FHIR version is the monthly release filename's MMDDYYYY date.
pub fn validate_version(version: &str) -> Result<()> {
    let invalid = || Error("RxNorm version requires a valid MMDDYYYY release date".into());
    if version.len() != 8 || !version.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    let month = version[..2].parse::<u32>().map_err(|_| invalid())?;
    let day = version[2..4].parse::<u32>().map_err(|_| invalid())?;
    let year = version[4..].parse::<u32>().map_err(|_| invalid())?;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return Err(invalid()),
    };
    if year == 0 || day == 0 || day > days {
        Err(invalid())
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticTypeField {
    Tui,
    Stn,
    Sty,
}
impl SemanticTypeField {
    /// Closed SQL column inventory; never use a caller-provided identifier.
    pub fn column(&self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Stn => "stn",
            Self::Sty => "sty",
        }
    }
    /// Validate one bounded filter value, including the TUI or tree-number grammar.
    /// Source loaders can reuse identifier validation without interpreting filter syntax.
    pub fn valid(&self, value: &str) -> bool {
        if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
            return false;
        }
        match self {
            Self::Tui => {
                value.len() == 4
                    && value.starts_with('T')
                    && value.as_bytes()[1..].iter().all(u8::is_ascii_digit)
            }
            Self::Stn => {
                value.as_bytes()[0].is_ascii_uppercase()
                    && value[1..].split('.').all(|part| {
                        !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
                    })
            }
            Self::Sty => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipField {
    Rel,
    Rela,
}
impl RelationshipField {
    /// Closed identifiers for source-backed relationship predicates.
    pub fn column(&self) -> &'static str {
        match self {
            Self::Rel => "rel",
            Self::Rela => "rela",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelationshipTarget {
    Cui,
    Aui,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    Code(String),
    Values {
        property: String,
        values: Vec<String>,
    },
    SemanticTypes {
        field: SemanticTypeField,
        values: Vec<String>,
    },
    NormalizedNdc(Vec<String>),
    Relationship {
        field: RelationshipField,
        relationship: String,
        target: RelationshipTarget,
        values: Vec<String>,
    },
    Scope,
}
impl Filter {
    pub fn parse(filter: &crate::Filter) -> Result<Self> {
        let crate::Filter {
            property,
            op,
            value,
        } = filter;
        if matches!(property.as_str(), "concept" | "code") && op == "=" {
            require_code(value)?;
            return Ok(Self::Code(value.clone()));
        }
        if op == "="
            && matches!(
                (property.as_str(), value.as_str()),
                ("source-table", SOURCE_TABLE)
                    | ("rxnorm-status", "active")
                    | ("inactive", "false")
            )
        {
            return Ok(Self::Scope);
        }
        if matches!(property.as_str(), "TTY" | "SAB") && matches!(op.as_str(), "=" | "in") {
            let values = if op == "=" {
                vec![value.as_str()]
            } else {
                value.split(',').map(str::trim).take(257).collect()
            };
            if values.is_empty()
                || values.len() > 256
                || values.iter().any(|value| {
                    value.is_empty()
                        || value.len() > 255
                        || value.contains(',')
                        || value.chars().any(char::is_control)
                })
            {
                return Err(Error(
                    "RxNorm TTY/SAB filter requires 1..256 nonempty bounded values".into(),
                ));
            }
            return Ok(Self::Values {
                property: property.clone(),
                values: values.into_iter().map(str::to_owned).collect(),
            });
        }
        let relationship = if RELATION_TYPES.contains(&property.as_str()) {
            Some(RelationshipField::Rel)
        } else if RELATION_LABELS.contains(&property.as_str()) {
            Some(RelationshipField::Rela)
        } else {
            None
        };
        if let Some(field) = relationship {
            if !matches!(op.as_str(), "=" | "in") || value.chars().any(char::is_control) {
                return Err(Error(
                    "RxNorm relationships require = or in with CUI:/AUI: identifiers".into(),
                ));
            }
            let parts: Vec<_> = if op == "=" {
                vec![value.as_str()]
            } else {
                value.split(',').map(str::trim).take(257).collect()
            };
            if parts.is_empty() || parts.len() > 256 {
                return Err(Error(
                    "RxNorm relationship filter requires 1..256 targets".into(),
                ));
            }
            let mut target: Option<RelationshipTarget> = None;
            let mut values = Vec::with_capacity(parts.len());
            for part in parts {
                let (specified, identifier) =
                    if let Some((prefix, identifier)) = part.split_once(':') {
                        let specified = match prefix {
                            "CUI" => RelationshipTarget::Cui,
                            "AUI" => RelationshipTarget::Aui,
                            _ => {
                                return Err(Error(
                                    "RxNorm relationship targets require CUI: or AUI:".into(),
                                ))
                            }
                        };
                        (Some(specified), identifier)
                    } else {
                        (None, part)
                    };
                let selected = specified.or_else(|| target.clone()).ok_or_else(|| {
                    Error(
                        "RxNorm relationship target requires an explicit CUI: or AUI: prefix"
                            .into(),
                    )
                })?;
                if target
                    .as_ref()
                    .is_some_and(|previous| previous != &selected)
                {
                    return Err(Error(
                        "RxNorm relationship lists cannot mix CUI and AUI targets".into(),
                    ));
                }
                require_code(identifier)?;
                target = Some(selected);
                values.push(identifier.to_owned());
            }
            return Ok(Self::Relationship {
                field,
                relationship: property.clone(),
                target: target.expect("Nonempty relationship list"),
                values,
            });
        }
        if matches!(property.as_str(), "STY" | "NDC") && matches!(op.as_str(), "=" | "in") {
            if value.chars().any(char::is_control) {
                return Err(Error("Control character in RxNorm auxiliary filter".into()));
            }
            let parts = if op == "=" {
                vec![value.as_str()]
            } else {
                value.split(',').map(str::trim).take(257).collect()
            };
            if parts.is_empty() || parts.len() > 256 {
                return Err(Error(
                    "RxNorm auxiliary filter requires 1..256 values".into(),
                ));
            }
            if property == "NDC" {
                if parts.iter().any(|value| {
                    value.len() != 11 || !value.bytes().all(|byte| byte.is_ascii_digit())
                }) {
                    return Err(Error("RxNorm NDC requires exact normalized 11-digit values; formatting and padding are unsupported".into()));
                }
                return Ok(Self::NormalizedNdc(
                    parts.into_iter().map(str::to_owned).collect(),
                ));
            }
            let mut field: Option<SemanticTypeField> = None;
            let mut values = Vec::with_capacity(parts.len());
            for part in parts {
                let (specified, value) = if let Some((prefix, value)) = part.split_once(':') {
                    let specified = match prefix {
                        "TUI" => SemanticTypeField::Tui,
                        "STN" => SemanticTypeField::Stn,
                        "STY" => SemanticTypeField::Sty,
                        _ => {
                            return Err(Error(
                                "Unknown RxNorm STY qualifier; use TUI, STN or STY".into(),
                            ))
                        }
                    };
                    (Some(specified), value)
                } else {
                    (None, part)
                };
                let selected =
                    specified.unwrap_or_else(|| field.clone().unwrap_or(SemanticTypeField::Tui));
                if field.as_ref().is_some_and(|previous| previous != &selected) {
                    return Err(Error("RxNorm STY lists cannot mix qualifiers".into()));
                }
                if !selected.valid(value) || value.contains(':') {
                    return Err(Error("Invalid RxNorm semantic type value".into()));
                }
                field = Some(selected);
                values.push(value.to_owned());
            }
            return Ok(Self::SemanticTypes {
                field: field.expect("Nonempty semantic type list"),
                values,
            });
        }
        Err(Error(format!("Unsupported current-active RxNorm filter {property} {op}; historical knowledge and unsupported source semantics remain unavailable")))
    }
    pub fn scopes_active(&self) -> bool {
        matches!(self, Self::Scope | Self::Code(_))
    }
}

pub fn implicit(canonical: &Canonical) -> Result<Option<serde_json::Value>> {
    if canonical.url == VALUE_SET {
        let mut selection = serde_json::json!({"system":SYSTEM});
        if let Some(version) = &canonical.version {
            selection["version"] = version.clone().into();
        }
        let mut resource = serde_json::json!({"resourceType":"ValueSet","url":VALUE_SET,
            "status":"active","compose":{"include":[selection]}});
        if let Some(version) = &canonical.version {
            resource["version"] = version.clone().into();
        }
        Ok(Some(resource))
    } else if canonical.url.starts_with(&format!("{VALUE_SET}/")) {
        Err(Error(
            "RxNorm implicit subsets require a supported provider".into(),
        ))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rxnorm_identity_and_release_dates_are_not_source_or_calendar_aliases() {
        for code in ["1", "310965", "999999999999999999"] {
            require_code(code).unwrap();
        }
        for code in [
            "",
            "0",
            "0310965",
            "RX310965",
            "1|2",
            "-1",
            "1.0",
            "1234567890123456789",
        ] {
            assert!(require_code(code).is_err());
        }
        for version in ["03022026", "02292024", "12312026"] {
            validate_version(version).unwrap();
        }
        for version in [
            "20260302",
            "02292025",
            "02302024",
            "13012026",
            "00012026",
            "01010000",
            "03022026x",
        ] {
            assert!(validate_version(version).is_err(), "{version}");
        }
    }

    #[test]
    fn rxnorm_filters_keep_scope_explicit_and_atom_values_exact() {
        let parse = |property: &str, op: &str, value: &str| {
            Filter::parse(&crate::Filter {
                property: property.into(),
                op: op.into(),
                value: value.into(),
            })
        };
        for (property, value) in [
            ("source-table", SOURCE_TABLE),
            ("rxnorm-status", "active"),
            ("inactive", "false"),
            ("code", "310965"),
        ] {
            assert!(parse(property, "=", value).unwrap().scopes_active());
        }
        assert_eq!(
            parse("TTY", "in", "SCD, PSN").unwrap(),
            Filter::Values {
                property: "TTY".into(),
                values: vec!["SCD".into(), "PSN".into()]
            }
        );
        assert!(!parse("SAB", "=", "VANDF").unwrap().scopes_active());
        assert!(!parse("RO", "=", "CUI:1").unwrap().scopes_active());
        for (property, op, value) in [
            ("inactive", "=", "true"),
            ("RO", "=", "CUI:0"),
            ("TTY", "regex", ".*"),
            ("TTY", "in", "SCD,,SBD"),
            ("SAB", "=", ""),
        ] {
            assert!(parse(property, op, value).is_err());
        }
        assert!(parse("TTY", "in", &vec!["SCD"; 257].join(",")).is_err());
    }

    #[test]
    fn auxiliary_filters_parse_closed_uniform_qualifiers_and_exact_ndc() {
        let parse = |property: &str, op: &str, value: &str| {
            Filter::parse(&crate::Filter {
                property: property.into(),
                op: op.into(),
                value: value.into(),
            })
        };
        for value in ["T109,T121", "TUI:T109,T121", "TUI:T109,TUI:T121"] {
            assert_eq!(
                parse("STY", "in", value).unwrap(),
                Filter::SemanticTypes {
                    field: SemanticTypeField::Tui,
                    values: vec!["T109".into(), "T121".into()]
                }
            );
        }
        assert_eq!(
            parse("STY", "in", "STN:A1.2,A1.3").unwrap(),
            Filter::SemanticTypes {
                field: SemanticTypeField::Stn,
                values: vec!["A1.2".into(), "A1.3".into()]
            }
        );
        assert_eq!(
            parse("STY", "=", "STY:Élément organique").unwrap(),
            Filter::SemanticTypes {
                field: SemanticTypeField::Sty,
                values: vec!["Élément organique".into()]
            }
        );
        assert_eq!(
            parse("STY", "=", "STY:Amino Acid, Peptide, or Protein").unwrap(),
            Filter::SemanticTypes {
                field: SemanticTypeField::Sty,
                values: vec!["Amino Acid, Peptide, or Protein".into()]
            }
        );
        assert_eq!(SemanticTypeField::Tui.column(), "tui");
        assert_eq!(SemanticTypeField::Stn.column(), "stn");
        assert_eq!(SemanticTypeField::Sty.column(), "sty");
        let ndc = parse("NDC", "in", "01234567890,00000123456").unwrap();
        assert_eq!(
            ndc,
            Filter::NormalizedNdc(vec!["01234567890".into(), "00000123456".into()])
        );
        assert!(!ndc.scopes_active());
        assert!(!parse("STY", "=", "T109").unwrap().scopes_active());
        for value in [
            "",
            "T12",
            "t109",
            "T109,",
            "TUI:T109,STN:A1.2",
            "T109,STY:Name",
            "BAD:T109",
            "STN:A",
            "STN:A1..2",
            "STN:α1.2",
            "STN:A1.2;DROP",
            "STY:",
            "STY:Name:T109",
            "STY:Name\n",
        ] {
            assert!(parse("STY", "in", value).is_err(), "{value}");
        }
        for value in [
            "1234567890",
            "123456789012",
            "01234-5678-90",
            "0123456789X",
            "",
        ] {
            assert!(parse("NDC", "=", value).is_err());
        }
        for property in ["STY", "NDC"] {
            assert!(parse(property, "regex", ".*").is_err());
            assert!(parse(property, "in", &vec!["T109"; 257].join(",")).is_err());
        }
        assert!(parse("STY", "=", &format!("STY:{}", "x".repeat(256))).is_err());
        // Name text is a value, never an identifier or executable SQL.
        assert_eq!(
            parse("STY", "=", "STY:' OR true --").unwrap(),
            Filter::SemanticTypes {
                field: SemanticTypeField::Sty,
                values: vec!["' OR true --".into()]
            }
        );
    }

    #[test]
    fn relationship_filters_have_closed_fields_and_explicit_typed_targets() {
        let parse = |property: &str, op: &str, value: &str| {
            Filter::parse(&crate::Filter {
                property: property.into(),
                op: op.into(),
                value: value.into(),
            })
        };
        for name in RELATION_TYPES.iter().chain(RELATION_LABELS) {
            let filter = parse(name, "=", "CUI:111").unwrap();
            assert!(!filter.scopes_active());
            assert_eq!(
                filter,
                Filter::Relationship {
                    field: if RELATION_TYPES.contains(name) {
                        RelationshipField::Rel
                    } else {
                        RelationshipField::Rela
                    },
                    relationship: (*name).into(),
                    target: RelationshipTarget::Cui,
                    values: vec!["111".into()],
                }
            );
        }
        assert_eq!(
            parse("has_ingredient", "in", "AUI:111,222,AUI:333").unwrap(),
            Filter::Relationship {
                field: RelationshipField::Rela,
                relationship: "has_ingredient".into(),
                target: RelationshipTarget::Aui,
                values: vec!["111".into(), "222".into(), "333".into()],
            }
        );
        assert_eq!(RelationshipField::Rel.column(), "rel");
        assert_eq!(RelationshipField::Rela.column(), "rela");
    }

    #[test]
    fn malformed_relationship_queries_never_become_empty_successes() {
        let parse = |property: &str, op: &str, value: &str| {
            Filter::parse(&crate::Filter {
                property: property.into(),
                op: op.into(),
                value: value.into(),
            })
        };
        for value in [
            "",
            "111",
            "CUI:",
            "cui:111",
            "RXCUI:111",
            "CUI:0",
            "CUI:0111",
            "CUI:C123",
            "AUI:A123",
            "CUI:111,",
            "CUI:111,,222",
            "CUI:111,AUI:222",
            "AUI:111,CUI:222",
            "CUI:111\n",
            "CUI:111') OR true --",
            "CUI:111:CUI",
            "CUI:1234567890123456789",
        ] {
            assert!(parse("RO", "in", value).is_err(), "{value}");
        }
        assert!(parse("RO", "=", "CUI:111,222").is_err());
        assert!(parse("RO", "in", &vec!["CUI:111"; 257].join(",")).is_err());
        assert!(parse("RO", "in", &vec!["CUI:111"; 256].join(",")).is_ok());
        for (property, op) in [
            ("RO", "regex"),
            ("isa", "is-a"),
            ("has_ingredient", "exists"),
            ("ro", "="),
            ("RQ", "="),
            ("unknown_relation", "="),
            ("rel;DROP TABLE concepts", "="),
        ] {
            assert!(parse(property, op, "CUI:111").is_err(), "{property} {op}");
        }
    }

    #[test]
    fn rxnorm_implicit_whole_definition_does_not_assert_enumerability() {
        let resource = implicit(&Canonical::parse(VALUE_SET, Some("03022026")).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(resource["compose"]["include"][0]["version"], "03022026");
        assert!(resource["compose"]["include"][0].get("filter").is_none());
        assert!(implicit(&Canonical::parse("urn:other", None).unwrap())
            .unwrap()
            .is_none());
        assert!(
            implicit(&Canonical::parse(&format!("{VALUE_SET}/unknown"), None).unwrap()).is_err()
        );
    }
}
