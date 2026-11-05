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

#[test]
fn declared_r4_status_policy_filters_retired_without_guessing_custom_lifecycles() {
    let source = json!({"url":"urn:status","content":"complete","property":[{"code":"status","uri":"http://hl7.org/fhir/concept-properties#status"}],"concept":[
        {"code":"active","property":[{"code":"status","valueCode":"active"}]},
        {"code":"experimental","property":[{"code":"status","valueCode":"experimental"}]},
        {"code":"deprecated","property":[{"code":"status","valueCode":"deprecated"}]},
        {"code":"retired","property":[{"code":"status","valueCode":"retired"}]}
    ]});
    let prepared =
        PreparedSystem::prepare_for_r4(serde_json::from_value(source.clone()).unwrap()).unwrap();
    let mut snapshot = Snapshot::default();
    snapshot.systems.insert(canonical("urn:status"), prepared);
    snapshot.value_sets.insert(
        canonical("urn:set"),
        serde_json::from_value(json!({"inactive":false,"include":[{"system":"urn:status"}]}))
            .unwrap(),
    );
    let members = snapshot.expand(&canonical("urn:set")).unwrap();
    assert_eq!(members.len(), 3);
    assert!(members.values().any(|member| member.code == "deprecated"));
    assert!(!members.values().any(|member| member.code == "retired"));
    snapshot.value_sets.insert(canonical("urn:set"),serde_json::from_value(json!({"include":[{"system":"urn:status","filter":[{"property":"status","op":"=","value":"retired"}]}]})).unwrap());
    let members = snapshot.expand(&canonical("urn:set")).unwrap();
    assert_eq!(members.len(), 1);
    assert!(members.values().next().unwrap().inactive);
    for change in ["undeclared", "custom", "unknown", "transition", "conflict"] {
        let mut invalid = source.clone();
        match change {
            "undeclared" => {
                invalid.as_object_mut().unwrap().remove("property");
            }
            "custom" => invalid["property"][0]["uri"] = json!("urn:custom-status"),
            "unknown" => invalid["concept"][0]["property"][0]["valueCode"] = json!("review-needed"),
            "transition" => invalid["concept"][0]["property"]
                .as_array_mut()
                .unwrap()
                .push(json!({"code":"retirementDate","valueDateTime":"2025-01-01"})),
            "conflict" => invalid["concept"][3]["property"]
                .as_array_mut()
                .unwrap()
                .push(json!({"code":"inactive","valueBoolean":false})),
            _ => unreachable!(),
        }
        assert!(
            PreparedSystem::prepare_for_r4(serde_json::from_value(invalid).unwrap()).is_err(),
            "{change}"
        );
    }
}

#[test]
fn graph_validation_preserves_shortest_depth_semantics_in_dense_dags() {
    let concepts=(0..40).map(|code| json!({"code":code.to_string(),"property":(0..code).map(|parent|json!({"code":"parent","valueCode":parent.to_string()})).collect::<Vec<_>>() })).collect::<Vec<_>>();
    let system=PreparedSystem::prepare(serde_json::from_value(json!({"url":"urn:dense","content":"complete","hierarchyMeaning":"is-a","concept":concepts})).unwrap()).unwrap();
    assert_eq!(system.ancestor_depths("39").unwrap().len(), 39);
    assert!(system
        .ancestor_depths("39")
        .unwrap()
        .values()
        .all(|depth| *depth == 1));
    let incomplete = json!({"url":"urn:fragment","content":"fragment","concept":[{"code":"known","property":[{"code":"child","valueCode":"not-in-fragment"}]}]});
    assert!(
        PreparedSystem::prepare_for_import(serde_json::from_value(incomplete).unwrap()).is_ok()
    );
    let complete = json!({"url":"urn:dangling","content":"complete","concept":[{"code":"known","property":[{"code":"child","valueCode":"unknown"}]}]});
    assert!(
        PreparedSystem::prepare(serde_json::from_value(complete).unwrap())
            .unwrap_err()
            .0
            .contains("Unknown hierarchy child")
    );
}

#[test]
fn shared_valueset_diamonds_do_not_re_evaluate_an_exponential_number_of_paths() {
    let mut state = snapshot(json!({"include":[{"system":"urn:codes","concept":[{"code":"a"}]}]}));
    for layer in 0..24 {
        for branch in ["a", "b"] {
            let imports = if layer == 0 {
                vec!["urn:set".to_owned()]
            } else {
                vec![
                    format!("urn:diamond:{}:a", layer - 1),
                    format!("urn:diamond:{}:b", layer - 1),
                ]
            };
            state.value_sets.insert(
                canonical(&format!("urn:diamond:{layer}:{branch}")),
                serde_json::from_value(json!({"include":[{"valueSet":imports}]})).unwrap(),
            );
        }
    }
    let result = state.expand(&canonical("urn:diamond:23:a")).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result.values().next().unwrap().code, "a");
}

#[test]
fn memoized_valuesets_cannot_bypass_import_depth_limits() {
    let mut state = snapshot(json!({"include":[{"system":"urn:codes"}]}));
    for index in 0..20 {
        let previous = if index == 0 {
            "urn:set".to_owned()
        } else {
            format!("urn:short:{}", index - 1)
        };
        state.value_sets.insert(
            canonical(&format!("urn:short:{index}")),
            serde_json::from_value(json!({"include":[{"valueSet":[previous]}]})).unwrap(),
        );
        let previous = if index == 0 {
            "urn:short:19".to_owned()
        } else {
            format!("urn:long:{}", index - 1)
        };
        state.value_sets.insert(
            canonical(&format!("urn:long:{index}")),
            serde_json::from_value(json!({"include":[{"valueSet":[previous]}]})).unwrap(),
        );
    }
    state.value_sets.insert(
        canonical("urn:root"),
        serde_json::from_value(
            json!({"include":[{"valueSet":["urn:short:19"]},{"valueSet":["urn:long:19"]}]}),
        )
        .unwrap(),
    );
    assert!(state
        .expand(&canonical("urn:root"))
        .unwrap_err()
        .0
        .contains("depth"));
}

#[test]
fn r4_scalar_membership_regex_and_direct_child_filters_are_typed_and_bounded() {
    let definition = json!({"url":"urn:filters","caseSensitive":false,"content":"complete","hierarchyMeaning":"is-a","property":[{"code":"tag"},{"code":"rank"}],"concept":[
        {"code":"Root","concept":[{"code":"Alpha","property":[{"code":"tag","valueCode":"x"},{"code":"tag","valueString":"yy"},{"code":"rank","valueInteger":2}],"concept":[{"code":"Grandchild"}]},{"code":"Beta","property":[{"code":"tag","valueCode":"z"}]}]},
        {"code":"Greekα"}
    ]});
    let mut state = Snapshot::default();
    state.systems.insert(
        canonical("urn:filters"),
        PreparedSystem::prepare_for_r4(serde_json::from_value(definition.clone()).unwrap())
            .unwrap(),
    );
    for (property, op, value, expected) in [
        ("code", "regex", "alpha", vec!["Alpha"]),
        ("code", "regex", "(alpha|beta)", vec!["Alpha", "Beta"]),
        ("tag", "regex", "y", vec![]),
        ("tag", "regex", "y+", vec!["Alpha"]),
        ("code", "in", "alpha,beta", vec!["Alpha", "Beta"]),
        (
            "code",
            "not-in",
            "root,alpha,beta,greekα",
            vec!["Grandchild"],
        ),
        ("rank", "in", "2,3", vec!["Alpha"]),
        ("concept", "child-of", "root", vec!["Alpha", "Beta"]),
        (
            "concept",
            "is-a",
            "root",
            vec!["Alpha", "Beta", "Grandchild", "Root"],
        ),
        (
            "concept",
            "generalizes",
            "grandchild",
            vec!["Alpha", "Grandchild", "Root"],
        ),
    ] {
        state.value_sets.insert(canonical("urn:set"),serde_json::from_value(json!({"include":[{"system":"urn:filters","filter":[{"property":property,"op":op,"value":value}]}]})).unwrap());
        let members = state.expand(&canonical("urn:set")).unwrap();
        assert_eq!(
            members
                .values()
                .map(|member| member.code.as_str())
                .collect::<Vec<_>>(),
            expected,
            "{property} {op} {value}"
        );
    }
    for expression in ["(", "(?<=a)b", "a{100000000}"] {
        state.value_sets.insert(canonical("urn:set"),serde_json::from_value(json!({"include":[{"system":"urn:filters","filter":[{"property":"code","op":"regex","value":expression}]}]})).unwrap());
        assert!(state.expand(&canonical("urn:set")).is_err(), "{expression}");
    }
    let too_long = "a".repeat(1025);
    state.value_sets.insert(canonical("urn:set"),serde_json::from_value(json!({"include":[{"system":"urn:filters","filter":[{"property":"code","op":"regex","value":too_long}]}]})).unwrap());
    assert!(state.expand(&canonical("urn:set")).is_err());
    // The conservative engine entry point retains its explicit unsupported-filter policy.
    state.systems.insert(
        canonical("urn:filters"),
        PreparedSystem::prepare(serde_json::from_value(definition).unwrap()).unwrap(),
    );
    assert!(state
        .expand(&canonical("urn:set"))
        .unwrap_err()
        .0
        .contains("Unsupported filter"));
}

#[test]
fn r4_boolean_properties_use_formal_meanings_and_false_defaults_without_relabeling_custom_data() {
    for (code, uri, meaningful) in [
        (
            "not-selectable",
            "http://hl7.org/fhir/concept-properties#notSelectable",
            true,
        ),
        (
            "notSelectable",
            "http://hl7.org/fhir/concept-properties#notSelectableX",
            false,
        ),
    ] {
        let source = json!({"url":"urn:flags","content":"complete","property":[{"code":code,"uri":uri,"type":"boolean"}],"concept":[{"code":"Unknown"},{"code":"Selectable","property":[{"code":code,"valueBoolean":false}]},{"code":"Grouping","property":[{"id":"source-flag","extension":[{"url":"urn:source","valueString":"retained"}],"code":code,"valueBoolean":true}]}]});
        let mut state = Snapshot::default();
        state.systems.insert(
            canonical("urn:flags"),
            PreparedSystem::prepare_for_r4(serde_json::from_value(source.clone()).unwrap())
                .unwrap(),
        );
        state.value_sets.insert(
            canonical("urn:set"),
            serde_json::from_value(json!({"include":[{"system":"urn:flags"}]})).unwrap(),
        );
        let members = state.expand(&canonical("urn:set")).unwrap();
        assert_eq!(
            members
                .values()
                .find(|member| member.code == "Grouping")
                .unwrap()
                .abstract_code,
            meaningful
        );
        state.value_sets.insert(canonical("urn:set"),serde_json::from_value(json!({"include":[{"system":"urn:flags","filter":[{"property":code,"op":"=","value":"false"}]}]})).unwrap());
        let members = state.expand(&canonical("urn:set")).unwrap();
        assert_eq!(members.len(), if meaningful { 2 } else { 1 });
        let mut malformed = source;
        malformed["concept"][2]["property"][0] = json!({"code":code,"valueString":"true"});
        if meaningful {
            assert!(
                PreparedSystem::prepare_for_r4(serde_json::from_value(malformed).unwrap()).is_err()
            );
        }
    }
}

#[test]
fn code_knowledge_retains_selected_identity_case_and_incomplete_source_uncertainty() {
    for (content, complete) in [
        ("complete", true),
        ("fragment", false),
        ("not-present", false),
    ] {
        let source = json!({"url":"urn:knowledge","version":"edition","content":content,"caseSensitive":false,"concept":[{"code":"Canonical"}]});
        let prepared =
            PreparedSystem::prepare_for_import(serde_json::from_value(source).unwrap()).unwrap();
        let index = prepared.into_code_index();
        assert_eq!(
            index.canonical,
            Canonical::parse("urn:knowledge", Some("edition")).unwrap()
        );
        assert_eq!(index.complete(), complete);
        assert_eq!(index.resolve("canonical"), Some("Canonical"));
        assert_eq!(index.resolve("missing"), None);
        assert!(index.memory_charge() > "Canonical".len());
    }
}

#[test]
fn finite_r4_properties_preserve_declared_types_and_reject_ambiguous_values() {
    let valid = json!({"url":"urn:typed","content":"complete","property":[{"code":"rank","type":"integer"}],"concept":[{"code":"a","property":[{"code":"rank","id":"element-id","extension":[{"url":"urn:label","valueString":"metadata"}],"valueInteger":7}]}]});
    assert!(PreparedSystem::prepare_for_r4(serde_json::from_value(valid.clone()).unwrap()).is_ok());
    for value in [
        json!({"valueString":"7"}),
        json!({"valueInteger":2147483648_i64}),
        json!({"valueInteger":7,"valueBoolean":true}),
        json!({"valueInteger":"7"}),
        json!({"valueExpression":{"expression":"7"}}),
    ] {
        let mut source = valid.clone();
        let mut property = value.as_object().unwrap().clone();
        property.insert("code".into(), json!("rank"));
        source["concept"][0]["property"][0] = property.into();
        assert!(PreparedSystem::prepare_for_r4(serde_json::from_value(source).unwrap()).is_err());
    }
    let mut source = valid;
    source["property"][0]["type"] = json!("unsupported-type");
    assert!(PreparedSystem::prepare_for_r4(serde_json::from_value(source).unwrap()).is_err());
}

#[test]
fn integer_filter_admission_does_not_depend_on_whether_concepts_are_present() {
    let source = json!({"url":"urn:typed","content":"complete","property":[{"code":"rank","type":"integer"}],"concept":[]});
    let mut snapshot = Snapshot::default();
    snapshot.systems.insert(
        canonical("urn:typed"),
        PreparedSystem::prepare_for_r4(serde_json::from_value(source).unwrap()).unwrap(),
    );
    for op in ["=", "in", "not-in"] {
        for (value, valid) in [
            ("-2147483648", true),
            ("2147483648", false),
            ("not-an-integer", false),
        ] {
            snapshot.value_sets.insert(canonical("urn:set"),serde_json::from_value(json!({"include":[{"system":"urn:typed","filter":[{"property":"rank","op":op,"value":value}]}]})).unwrap());
            let result = snapshot.expand(&canonical("urn:set"));
            assert_eq!(result.is_ok(), valid, "{op} {value}: {result:?}");
            if let Ok(members) = result {
                assert!(members.is_empty());
            }
        }
    }
}

#[test]
fn standard_property_aliases_cannot_change_types_or_hide_conflicting_boolean_values() {
    let source = json!({"url":"urn:typed-flags","content":"fragment","property":[{"code":"flag","uri":"http://hl7.org/fhir/concept-properties#notSelectable","type":"string"}],"concept":[]});
    assert!(
        PreparedSystem::prepare_for_import(serde_json::from_value(source.clone()).unwrap()).is_ok()
    );
    assert!(
        PreparedSystem::prepare_for_r4_source(serde_json::from_value(source).unwrap()).is_err()
    );
    let source = json!({"url":"urn:typed-flags","content":"fragment","property":[{"code":"first","uri":"http://hl7.org/fhir/concept-properties#notSelectable","type":"boolean"},{"code":"second","uri":"http://hl7.org/fhir/concept-properties#notSelectable","type":"boolean"}],"concept":[{"code":"A","property":[{"code":"first","valueBoolean":false},{"code":"second","valueBoolean":true}]}]});
    assert!(
        PreparedSystem::prepare_for_r4_source(serde_json::from_value(source).unwrap()).is_err()
    );
}

#[test]
fn graph_admission_agrees_with_independent_shortest_paths_on_sparse_dense_and_invalid_inputs() {
    fn reference(parents: &[Vec<usize>]) -> bool {
        let count = parents.len();
        if parents.iter().flatten().any(|&parent| parent >= count) {
            return false;
        }
        for start in 0..count {
            let mut distances = vec![None; count];
            distances[start] = Some(0);
            let mut pending = std::collections::VecDeque::from([start]);
            while let Some(node) = pending.pop_front() {
                for &parent in &parents[node] {
                    if parent == start {
                        return false;
                    }
                    if distances[parent].is_none() {
                        distances[parent] = Some(distances[node].unwrap() + 1);
                        pending.push_back(parent);
                    }
                }
            }
            if distances
                .into_iter()
                .flatten()
                .any(|depth| depth > MAX_DEPTH)
            {
                return false;
            }
        }
        true
    }
    for seed in 0..160 {
        let count = 2 + seed % 40;
        let mut parents = vec![Vec::new(); count];
        for (child, links) in parents.iter_mut().enumerate().skip(1) {
            match seed % 4 {
                0 => links.extend(0..child),
                1 => links.push(child - 1),
                _ => {
                    links.push((child - 1) / 2);
                    links.push((child * (seed + 3) + seed) % child);
                    links.sort_unstable();
                    links.dedup();
                }
            }
        }
        if seed % 7 == 0 {
            parents[0].push(count - 1);
        }
        if seed % 11 == 0 {
            parents[count - 1].push(count);
        }
        let expected = reference(&parents);
        let source = json!({"url":format!("urn:graph-parity:{seed}"),"content":"complete","caseSensitive":true,"hierarchyMeaning":"is-a","property":[{"code":"parent","uri":"http://hl7.org/fhir/concept-properties#parent","type":"code"}],"concept":parents.iter().enumerate().map(|(index,links)|json!({"code":format!("N{index}"),"property":links.iter().map(|parent|json!({"code":"parent","valueCode":format!("N{parent}")})).collect::<Vec<_>>()})).collect::<Vec<_>>()});
        let result = PreparedSystem::prepare_for_r4(serde_json::from_value(source).unwrap());
        assert_eq!(result.is_ok(), expected, "seed {seed}: {result:?}");
    }
}
