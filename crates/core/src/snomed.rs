//! Keep unsupported SNOMED expression handling separate from scalar code membership.
use crate::{Error, Result};
pub mod ecl;
pub use ecl::Ecl;

/// A complete RF2 identifier projection is not a post-coordination evaluator.
pub fn reject_expression(code: &str) -> Result<()> {
    if code.bytes().any(|b| b":+|{}=(),".contains(&b)) {
        return Err(Error(
            "SNOMED expression/post-coordination evaluation is unsupported".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expressions_are_not_classified_as_invalid_scalar_codes() {
        for code in [
            "100001:100002=100003",
            "100001+100002",
            "100001 |fixture|",
            "100001:{100002=100003}",
        ] {
            assert!(reject_expression(code).is_err());
        }
        for code in ["100001", "999999999999", "missing"] {
            assert!(reject_expression(code).is_ok()); // Existence is the projection's job.
        }
    }
}

pub const SYSTEM: &str = "http://snomed.info/sct";
pub const INTERNATIONAL: &str = "900000000000207008";
pub const SIMPLE_REFSET_ROOT: &str = "446609009";

/// A reference-set selector is a scalar SCTID, not a URL, list or expression.
pub fn require_refset_code(code: &str) -> Result<()> {
    if (6..=18).contains(&code.len())
        && !code.starts_with('0')
        && code.bytes().all(|byte| byte.is_ascii_digit())
    {
        Ok(())
    } else {
        Err(Error(
            "Expected a nonzero 6..18 digit SNOMED refset identifier".into(),
        ))
    }
}

/// A native filter describes a relation; it never enumerates a clinical release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeFilter {
    Constraint(Ecl),
    Refset(String),
    Hierarchy {
        anchor: String,
        ancestors: bool,
        strict: bool,
        negate: bool,
    },
    Code(String),
    Property {
        code: String,
        value: String,
    },
}
impl NativeFilter {
    pub fn parse(filter: &crate::Filter) -> Result<Self> {
        let crate::Filter {
            property,
            op,
            value,
        } = filter;
        if property == "constraint" && op == "=" {
            return Ok(Self::Constraint(Ecl::parse(value)?));
        }
        if property == "concept" && op == "in" {
            require_refset_code(value)?;
            return Ok(Self::Refset(value.clone()));
        }
        if property == "concept"
            && matches!(
                op.as_str(),
                "is-a" | "descendent-of" | "is-not-a" | "generalizes"
            )
        {
            identifier(value)?;
            return Ok(Self::Hierarchy {
                anchor: value.clone(),
                ancestors: op == "generalizes",
                strict: op == "descendent-of",
                negate: op == "is-not-a",
            });
        }
        if op == "=" && matches!(property.as_str(), "concept" | "code") {
            reject_expression(value)?;
            return Ok(Self::Code(value.clone()));
        }
        if op == "="
            && (property == "inactive"
                || property == "moduleId"
                || property == "definitionStatusId"
                || property == "effectiveTime"
                || is_identifier(property))
        {
            if property == "inactive" && value != "true" && value != "false" {
                return Err(Error("inactive expects true or false".into()));
            }
            if is_identifier(property) {
                identifier(value)?;
            }
            return Ok(Self::Property {
                code: property.clone(),
                value: value.clone(),
            });
        }
        Err(Error(format!(
            "Unsupported native SNOMED filter {property} {op}"
        )))
    }
}
fn is_identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 18 && s.bytes().all(|b| b.is_ascii_digit())
}
fn identifier(s: &str) -> Result<()> {
    if !is_identifier(s) {
        return Err(Error("Expected a scalar SNOMED identifier".into()));
    }
    Ok(())
}

/// Edition selectors are scalar SCTIDs, never SQL patterns or arbitrary version prefixes.
pub fn is_edition_uri(version: &str) -> bool {
    version
        .strip_prefix(&format!("{SYSTEM}/"))
        .is_some_and(is_identifier)
}

/// Storage-independent definition of the supported implicit SNOMED set forms.
/// An absent system version is resolved by the provider's International default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplicitValueSet {
    pub system_version: Option<String>,
    pub ancestor: Option<String>,
    pub constraint: Option<Ecl>,
    pub refset: Option<String>,
}
impl ImplicitValueSet {
    /// Render the same FHIR template used for stored/inline compose evaluation.
    pub fn resource(&self, canonical: &crate::Canonical) -> serde_json::Value {
        let mut selection = serde_json::json!({"system": SYSTEM});
        if let Some(version) = &self.system_version {
            selection["version"] = version.clone().into();
        }
        if let Some(ancestor) = &self.ancestor {
            selection["filter"] = serde_json::json!([
                {"property":"concept","op":"is-a","value":ancestor}
            ]);
        }
        if let Some(constraint) = &self.constraint {
            selection["filter"] = serde_json::json!([
                {"property":"constraint","op":"=","value":constraint.source()}
            ]);
        }
        if let Some(refset) = &self.refset {
            selection["filter"] = serde_json::json!([
                {"property":"concept","op":"in","value":refset}
            ]);
        }
        let mut resource = serde_json::json!({"resourceType":"ValueSet","url":canonical.url,
            "status":"active","compose":{"include":[selection]}});
        if let Some(version) = &canonical.version {
            resource["version"] = version.clone().into();
        }
        resource
    }
}

/// Recognize only the supported R4 implicit grammar without building a JSON resource.
/// Bounded ECL and declared concept refsets use the general provider.
pub fn implicit_definition(canonical: &crate::Canonical) -> Result<Option<ImplicitValueSet>> {
    let Some((base, query)) = canonical.url.split_once('?') else {
        return Ok(None);
    };
    if base != SYSTEM && !base.starts_with(&format!("{SYSTEM}/")) {
        return Ok(None);
    }
    let suffix = base.strip_prefix(SYSTEM).unwrap();
    if !suffix.is_empty() {
        let parts: Vec<_> = suffix.trim_start_matches('/').split('/').collect();
        if !matches!(parts.as_slice(), [edition] if is_identifier(edition))
            && !matches!(parts.as_slice(), [edition, "version", date] if is_identifier(edition) && date.len()==8 && date.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(Error("Invalid SNOMED edition/version URI".into()));
        }
    }
    let mut system_version = (base != SYSTEM).then(|| base.to_owned());
    if let Some(version) = &canonical.version {
        if base != SYSTEM && version != base && !version.starts_with(&format!("{base}/version/")) {
            return Err(Error(
                "Conflicting implicit ValueSet edition/version".into(),
            ));
        }
        system_version = Some(version.clone());
    }
    let constraint = query
        .strip_prefix("fhir_vs=ecl/")
        .map(ecl::decode_implicit)
        .transpose()?;
    let refset = query
        .strip_prefix("fhir_vs=refset/")
        .map(|code| {
            require_refset_code(code)?;
            Ok::<_, Error>(code.to_owned())
        })
        .transpose()?;
    let ancestor = if query != "fhir_vs" && constraint.is_none() && refset.is_none() {
        let anchor = query.strip_prefix("fhir_vs=isa/").ok_or_else(|| {
            Error(
                "Unsupported SNOMED implicit query; global refset enumeration requires complete registry coverage"
                    .into(),
            )
        })?;
        identifier(anchor)?;
        Some(anchor.to_owned())
    } else {
        None
    };
    Ok(Some(ImplicitValueSet {
        system_version,
        ancestor,
        constraint,
        refset,
    }))
}

/// Compatibility wrapper for adapters that require a FHIR compose template.
pub fn implicit(canonical: &crate::Canonical) -> Result<Option<serde_json::Value>> {
    Ok(implicit_definition(canonical)?.map(|definition| definition.resource(canonical)))
}

#[cfg(test)]
mod native_tests {
    use super::*;
    use crate::{Canonical, Filter};
    #[test]
    fn declared_concept_refset_selectors_preserve_edition_and_active_member_semantics() {
        let filter = NativeFilter::parse(&Filter {
            property: "concept".into(),
            op: "in".into(),
            value: "100001".into(),
        })
        .unwrap();
        assert_eq!(filter, NativeFilter::Refset("100001".into()));
        let url = format!("{SYSTEM}/{INTERNATIONAL}/version/20260201?fhir_vs=refset/100001");
        let canonical = Canonical::parse(&url, None).unwrap();
        let definition = implicit_definition(&canonical).unwrap().unwrap();
        assert_eq!(definition.refset.as_deref(), Some("100001"));
        assert!(definition.constraint.is_none() && definition.ancestor.is_none());
        let resource = definition.resource(&canonical);
        assert_eq!(resource["url"], url);
        assert_eq!(
            resource["compose"]["include"][0]["filter"][0],
            serde_json::json!({"property":"concept","op":"in","value":"100001"})
        );
    }
    #[test]
    fn refset_namespace_is_scalar_and_never_implies_global_registry_coverage() {
        for code in [
            "",
            "1",
            "0100001",
            "100001,100002",
            "100001&extra=1",
            "100001/foo",
            "^100001",
            "100001 |label|",
        ] {
            assert!(require_refset_code(code).is_err(), "{code}");
            assert!(
                Canonical::parse(&format!("{SYSTEM}?fhir_vs=refset/{code}"), None)
                    .and_then(|canonical| implicit(&canonical))
                    .is_err()
            );
        }
        assert!(
            implicit(&Canonical::parse(&format!("{SYSTEM}?fhir_vs=refset"), None).unwrap())
                .is_err()
        );
    }
    #[test]
    fn typed_implicit_definitions_preserve_release_selection_and_ancestors() {
        let default = Canonical::parse(&format!("{SYSTEM}?fhir_vs"), None).unwrap();
        assert_eq!(
            implicit_definition(&default).unwrap().unwrap(),
            ImplicitValueSet {
                system_version: None,
                ancestor: None,
                constraint: None,
                refset: None,
            }
        );
        let edition = format!("{SYSTEM}/{INTERNATIONAL}");
        let version = format!("{edition}/version/20260201");
        for selector in [&edition, &version] {
            let canonical =
                Canonical::parse(&format!("{selector}?fhir_vs=isa/404684003"), None).unwrap();
            let definition = implicit_definition(&canonical).unwrap().unwrap();
            assert_eq!(
                definition.system_version.as_deref(),
                Some(selector.as_str())
            );
            assert_eq!(definition.ancestor.as_deref(), Some("404684003"));
        }
        // An explicit parameter wins over the unspecified/default or matching edition.
        for base in [SYSTEM, edition.as_str()] {
            let canonical = Canonical::parse(&format!("{base}?fhir_vs"), Some(&version)).unwrap();
            let definition = implicit_definition(&canonical).unwrap().unwrap();
            assert_eq!(definition.system_version.as_deref(), Some(version.as_str()));
            assert!(definition.ancestor.is_none());
            let resource = definition.resource(&canonical);
            assert_eq!(resource["version"], version);
            assert_eq!(resource["compose"]["include"][0]["version"], version);
            assert!(resource["compose"]["include"][0].get("filter").is_none());
        }
        let canonical = Canonical::parse(&format!("{version}?fhir_vs=isa/1"), None).unwrap();
        let resource = implicit_definition(&canonical)
            .unwrap()
            .unwrap()
            .resource(&canonical);
        assert!(resource.get("version").is_none());
        assert_eq!(resource["compose"]["include"][0]["filter"][0]["value"], "1");
        assert_eq!(resource["url"], canonical.url);
    }
    #[test]
    fn typed_implicit_definition_errors_are_not_empty_sets() {
        let version = format!("{SYSTEM}/{INTERNATIONAL}/version/20260201");
        let canonical = Canonical::parse(&format!("{version}?fhir_vs"), Some("other")).unwrap();
        assert!(implicit_definition(&canonical).is_err());
        for suffix in [
            "?fhir_vs=ecl/<<1",
            "?fhir_vs=refset",
            "?fhir_vs=isa/1&other=2",
            "?fhir_vs=isa/1:2=3",
            "?fhir_vs=isa/1234567890123456789",
            "/bad?fhir_vs",
        ] {
            let canonical = Canonical::parse(&format!("{SYSTEM}{suffix}"), None).unwrap();
            assert!(implicit_definition(&canonical).is_err(), "{suffix}");
        }
        let unrelated = Canonical::parse("urn:finite?fhir_vs", None).unwrap();
        assert!(implicit_definition(&unrelated).unwrap().is_none());
    }
    #[test]
    fn bounded_ecl_implicit_keeps_uri_and_routes_a_constraint_not_the_whole_set() {
        let version = format!("{SYSTEM}/{INTERNATIONAL}/version/20260201");
        let url = format!("{version}?fhir_vs=ecl/%3C%3C%20100000%20%7Clabel%20%2B%20%26%7C");
        let canonical = Canonical::parse(&url, None).unwrap();
        let definition = implicit_definition(&canonical).unwrap().unwrap();
        assert_eq!(definition.system_version.as_deref(), Some(version.as_str()));
        assert!(definition.ancestor.is_none());
        assert_eq!(
            definition.constraint.as_ref().unwrap().source(),
            "<< 100000 |label + &|"
        );
        let resource = definition.resource(&canonical);
        assert_eq!(resource["url"], url);
        assert_eq!(resource["compose"]["include"][0]["version"], version);
        assert_eq!(
            resource["compose"]["include"][0]["filter"][0]["property"],
            "constraint"
        );
        assert_eq!(
            resource["compose"]["include"][0]["filter"][0]["value"],
            "<< 100000 |label + &|"
        );
        assert!(NativeFilter::parse(&Filter {
            property: "constraint".into(),
            op: "=".into(),
            value: "<< 100000".into()
        })
        .is_ok());
    }
    #[test]
    fn implicit_grammar_versions_and_unsupported_are_explicit() {
        for url in [
            format!("{SYSTEM}?fhir_vs"),
            format!("{SYSTEM}/{INTERNATIONAL}/version/20260201?fhir_vs=isa/1"),
        ] {
            assert!(implicit(&Canonical::parse(&url, None).unwrap())
                .unwrap()
                .is_some());
        }
        for suffix in [
            "?fhir_vs=ecl/<<1",
            "?fhir_vs=refset/1",
            "?fhir_vs=isa/1&other=2",
            "/bad?fhir_vs",
        ] {
            assert!(
                implicit(&Canonical::parse(&format!("{SYSTEM}{suffix}"), None).unwrap()).is_err()
            );
        }
        assert!(is_edition_uri(&format!("{SYSTEM}/{INTERNATIONAL}")));
        for suffix in [
            "/%",
            "/_",
            "/",
            "/bad",
            "/900000000000207008/version/20260201",
        ] {
            assert!(!is_edition_uri(&format!("{SYSTEM}{suffix}")));
        }
        assert!(implicit(&Canonical::parse("urn:finite", None).unwrap())
            .unwrap()
            .is_none());
        assert!(implicit(
            &Canonical::parse(
                &format!("{SYSTEM}/{INTERNATIONAL}/version/20260201?fhir_vs"),
                Some("other")
            )
            .unwrap()
        )
        .is_err());
    }
    #[test]
    fn native_filters_preserve_direction_and_reject_unknown_operators() {
        let parse = |p: &str, o: &str, v: &str| {
            NativeFilter::parse(&Filter {
                property: p.into(),
                op: o.into(),
                value: v.into(),
            })
        };
        assert_eq!(
            parse("concept", "generalizes", "1").unwrap(),
            NativeFilter::Hierarchy {
                anchor: "1".into(),
                ancestors: true,
                strict: false,
                negate: false
            }
        );
        assert!(parse("concept", "in", "1").is_err());
        assert!(parse("constraint", "=", "<<1").is_err());
        assert!(parse("inactive", "=", "maybe").is_err());
        assert!(parse("123", "=", "456").is_ok());
    }
}
