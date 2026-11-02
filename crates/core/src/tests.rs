use super::*;
use serde_json::json;

fn system() -> PreparedSystem {
    PreparedSystem::prepare(serde_json::from_value(json!({
        "url":"urn:codes", "version":"1", "content":"complete", "hierarchyMeaning":"is-a",
        "concept":[{"code":"root","concept":[{"code":"a","display":"Alpha","property":[{"code":"class","valueCode":"x"}]},
            {"code":"b","property":[{"code":"inactive","valueBoolean":true}]}]}, {"code":"other"}]
    })).unwrap()).unwrap()
}
fn canonical(url: &str) -> Canonical {
    Canonical::parse(url, None).unwrap()
}
fn snapshot(compose: serde_json::Value) -> Snapshot {
    let mut s = Snapshot::default();
    s.systems.insert(canonical("urn:codes"), system());
    s.value_sets.insert(
        canonical("urn:set"),
        serde_json::from_value(compose).unwrap(),
    );
    s
}
fn codes(s: &Snapshot) -> Vec<String> {
    s.expand(&canonical("urn:set"))
        .unwrap()
        .into_values()
        .map(|m| m.code)
        .collect()
}

#[test]
fn canonical_versions_are_business_versions() {
    assert_eq!(
        Canonical::parse("urn:codes|1", Some("1"))
            .unwrap()
            .version
            .as_deref(),
        Some("1")
    );
    assert!(Canonical::parse("urn:codes|1", Some("2")).is_err());
    for url in ["", "urn:codes|", "urn:codes|1|2"] {
        assert!(Canonical::parse(url, None).is_err());
    }
}
#[test]
fn hierarchy_supports_nested_and_property_edges() {
    let s = system();
    assert_eq!(s.ancestors("a").unwrap(), BTreeSet::from(["root".into()]));
    assert!(s.ancestors("other").unwrap().is_empty());
    let p = PreparedSystem::prepare(
        serde_json::from_value(json!({"url":"urn:c","content":"complete",
        "concept":[{"code":"a","property":[{"code":"child","valueCode":"b"}]},{"code":"b"}]}))
        .unwrap(),
    )
    .unwrap();
    assert!(p.ancestors("b").unwrap().contains("a"));
}
#[test]
fn hierarchy_cycles_duplicates_and_unknown_parents_fail() {
    for concepts in [
        json!([{"code":"a"},{"code":"a"}]),
        json!([{"code":"a","property":[{"code":"parent","valueCode":"missing"}]}]),
        json!([{"code":"a","property":[{"code":"parent","valueCode":"b"}]},{"code":"b","property":[{"code":"parent","valueCode":"a"}]}]),
    ] {
        assert!(PreparedSystem::prepare(
            serde_json::from_value(json!({"url":"urn:c","content":"complete","concept":concepts}))
                .unwrap()
        )
        .is_err());
    }
}
#[test]
fn explicit_union_exclusion_and_deduplication() {
    let s = snapshot(
        json!({"include":[{"system":"urn:codes","concept":[{"code":"a"},{"code":"b"}]},
        {"system":"urn:codes","concept":[{"code":"a"}]}],"exclude":[{"system":"urn:codes","concept":[{"code":"b"}]}]}),
    );
    assert_eq!(codes(&s), ["a"]);
    let members = s.expand(&canonical("urn:set")).unwrap();
    assert_eq!(
        members.values().next().unwrap().version.as_deref(),
        Some("1")
    );
    assert_eq!(
        members.values().next().unwrap().display.as_deref(),
        Some("Alpha")
    );
}
#[test]
fn complete_system_and_inactive_selection() {
    let s = snapshot(json!({"inactive":false,"include":[{"system":"urn:codes"}]}));
    assert_eq!(codes(&s), ["a", "other", "root"]);
}
#[test]
fn imports_intersect_inside_selection_and_union_between_selections() {
    let mut s = snapshot(json!({"include":[{"valueSet":["urn:left","urn:right"]}]}));
    s.value_sets.insert(
        canonical("urn:left"),
        serde_json::from_value(
            json!({"include":[{"system":"urn:codes","concept":[{"code":"a"},{"code":"b"}]}]}),
        )
        .unwrap(),
    );
    s.value_sets.insert(
        canonical("urn:right"),
        serde_json::from_value(
            json!({"include":[{"system":"urn:codes","concept":[{"code":"b"},{"code":"other"}]}]}),
        )
        .unwrap(),
    );
    assert_eq!(codes(&s), ["b"]);
    s.value_sets.insert(canonical("urn:set"), serde_json::from_value(json!({"include":[{"system":"urn:codes","concept":[{"code":"a"}]}, {"valueSet":["urn:right"]}]})).unwrap());
    assert_eq!(codes(&s), ["a", "b", "other"]);
}
#[test]
fn system_constraint_intersects_imports_and_exclude_can_import() {
    let mut s = snapshot(
        json!({"include":[{"system":"urn:codes","concept":[{"code":"a"}],"valueSet":["urn:subset"]}]}),
    );
    s.value_sets.insert(
        canonical("urn:subset"),
        serde_json::from_value(
            json!({"include":[{"system":"urn:codes","concept":[{"code":"b"}]}]}),
        )
        .unwrap(),
    );
    assert!(codes(&s).is_empty());
    s.value_sets.insert(
        canonical("urn:set"),
        serde_json::from_value(
            json!({"include":[{"system":"urn:codes"}],"exclude":[{"valueSet":["urn:subset"]}]}),
        )
        .unwrap(),
    );
    assert_eq!(codes(&s), ["a", "other", "root"]);
}
#[test]
fn hierarchy_and_property_filters_are_conjunctive() {
    for (op, expected) in [
        ("is-a", vec!["a", "b", "root"]),
        ("descendent-of", vec!["a", "b"]),
        ("is-not-a", vec!["other"]),
    ] {
        let s = snapshot(
            json!({"include":[{"system":"urn:codes","filter":[{"property":"concept","op":op,"value":"root"}]}]}),
        );
        assert_eq!(codes(&s), expected);
    }
    let s = snapshot(
        json!({"include":[{"system":"urn:codes","filter":[{"property":"concept","op":"is-a","value":"root"},
        {"property":"class","op":"=","value":"x"}]}]}),
    );
    assert_eq!(codes(&s), ["a"]);
}
#[test]
fn unsupported_and_unresolved_selections_fail_instead_of_returning_empty() {
    for selection in [
        json!({}),
        json!({"concept":[{"code":"a"}]}),
        json!({"system":"urn:unknown"}),
        json!({"system":"urn:codes","concept":[{"code":"unknown"}]}),
        json!({"system":"urn:codes","filter":[{"property":"concept","op":"regex","value":".*"}]}),
        json!({"system":"urn:codes","filter":[{"property":"concept","op":"is-a","value":"missing"}]}),
        json!({"system":"urn:codes","filter":[{"property":"class","op":"exists","value":"yes"}]}),
    ] {
        assert!(snapshot(json!({"include":[selection]}))
            .expand(&canonical("urn:set"))
            .is_err());
    }
}
#[test]
fn cycles_and_depth_limits_are_errors() {
    let mut s = snapshot(json!({"include":[{"valueSet":["urn:set"]}]}));
    assert!(s.expand(&canonical("urn:set")).is_err());
    for n in 0..=MAX_DEPTH {
        s.value_sets.insert(
            canonical(&format!("urn:v{n}")),
            serde_json::from_value(json!({"include":[{"valueSet":[format!("urn:v{}",n+1)]}]}))
                .unwrap(),
        );
    }
    assert!(s
        .expand(&canonical("urn:v0"))
        .unwrap_err()
        .0
        .contains("depth"));
}
#[test]
fn explicit_case_insensitive_codes_resolve_to_canonical_spelling() {
    let mut s = snapshot(json!({"include":[{"system":"urn:codes","concept":[{"code":"A"}]}]}));
    s.systems
        .get_mut(&canonical("urn:codes"))
        .unwrap()
        .case_sensitive = false;
    assert_eq!(codes(&s), ["a"]);
}
#[test]
fn versioned_exclusion_does_not_remove_another_release() {
    let mut s = snapshot(
        json!({"include":[{"system":"urn:codes"}],"exclude":[{"system":"urn:codes","version":"2","concept":[{"code":"a"}]}]}),
    );
    let mut v2 = system();
    v2.canonical.version = Some("2".into());
    s.systems.insert(canonical("urn:codes|2"), v2);
    assert!(codes(&s).contains(&"a".to_owned()));
}

#[test]
fn typed_equality_is_not_json_string_comparison() {
    for property in [
        json!({"code":"p","valueBoolean":true}),
        json!({"code":"p","valueInteger":1}),
    ] {
        let expected = if property.get("valueBoolean").is_some() {
            "true"
        } else {
            "1"
        };
        let mut s = snapshot(
            json!({"include":[{"system":"urn:codes","filter":[{"property":"p","op":"=","value":expected}]}]}),
        );
        s.systems
            .get_mut(&canonical("urn:codes"))
            .unwrap()
            .concepts
            .get_mut("a")
            .unwrap()
            .property
            .push(serde_json::from_value(property).unwrap());
        assert_eq!(codes(&s), ["a"]);
    }
    for property in [
        json!({"code":"p","valueCoding":{"system":"urn:other","code":"x"}}),
        json!({"code":"p","valueDecimal":1.0}),
        json!({"code":"p","valueDateTime":"2026-01-01"}),
    ] {
        let mut s = snapshot(
            json!({"include":[{"system":"urn:codes","filter":[{"property":"p","op":"=","value":"1"}]}]}),
        );
        s.systems
            .get_mut(&canonical("urn:codes"))
            .unwrap()
            .concepts
            .get_mut("a")
            .unwrap()
            .property
            .push(serde_json::from_value(property).unwrap());
        assert!(s
            .expand(&canonical("urn:set"))
            .unwrap_err()
            .0
            .contains("not supported"));
    }
}

#[test]
fn unsupported_standard_property_meanings_are_not_ignored() {
    let alias = json!({"url":"urn:c","content":"complete","property":[
        {"code":"ancestor","uri":"http://hl7.org/fhir/concept-properties#parent","type":"code"}],
        "concept":[{"code":"a"}]});
    assert!(
        PreparedSystem::prepare(serde_json::from_value(alias).unwrap())
            .unwrap_err()
            .0
            .contains("aliases")
    );
    let retired = json!({"url":"urn:c","content":"complete","concept":[
        {"code":"a","property":[{"code":"status","valueCode":"retired"}]}]});
    assert!(
        PreparedSystem::prepare(serde_json::from_value(retired.clone()).unwrap())
            .unwrap_err()
            .0
            .contains("Lifecycle")
    );
    // Import can retain lifecycle data without pretending to evaluate its active-code policy.
    assert!(PreparedSystem::prepare_for_import(serde_json::from_value(retired).unwrap()).is_ok());
}

#[test]
fn prepared_membership_matches_expansion_and_survives_snapshot_drop() {
    let s = snapshot(
        json!({"include":[{"system":"urn:codes"}],"exclude":[{"system":"urn:codes","concept":[{"code":"b"}]}]}),
    );
    let plan = PreparedValueSet::prepare(&s, &canonical("urn:set")).unwrap();
    drop(s);
    assert!(plan.find("urn:codes", Some("1"), "a").is_some());
    for (system, version, code) in [
        ("urn:codes", Some("2"), "a"),
        ("urn:codes", None, "b"),
        ("urn:codes", None, "missing"),
        ("urn:other", None, "a"),
    ] {
        assert!(plan.find(system, version, code).is_none());
    }
    assert_eq!(plan.values().count(), 3);
    assert!(plan.memory_charge() > 0);
}
#[test]
fn prepared_index_preserves_case_policy() {
    let mut s = snapshot(json!({"include":[{"system":"urn:codes"}]}));
    s.systems
        .get_mut(&canonical("urn:codes"))
        .unwrap()
        .case_sensitive = false;
    let plan = PreparedValueSet::prepare(&s, &canonical("urn:set")).unwrap();
    assert_eq!(plan.find("urn:codes", Some("1"), "A").unwrap().code, "a");
    assert!(plan.find("URN:CODES", None, "a").is_none());
}
