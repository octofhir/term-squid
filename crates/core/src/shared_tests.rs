//! Shared preparations preserve the owned API, strict policy and source isolation.
use super::*;
use serde_json::{json, Value};
fn source() -> Value {
    json!({"url":"urn:shared","version":"1","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[{"code":"Root","concept":[{"code":"Alpha","display":"Alpha"},{"code":"Beta","display":"Beta"}]}]})
}
fn snapshot() -> (Snapshot, Canonical) {
    let root = Canonical::parse("urn:shared:set", None).unwrap();
    let mut snapshot = Snapshot::default();
    snapshot.value_sets.insert(root.clone(),serde_json::from_value(json!({"include":[{"system":"urn:shared","filter":[{"property":"concept","op":"is-a","value":"root"}]}],"exclude":[{"system":"urn:shared","concept":[{"code":"beta"}]}]})).unwrap());
    (snapshot, root)
}
#[test]
fn shared_prepared_sources_match_owned_composition_without_retaining_or_mutating_the_view() {
    let (mut snapshot, root) = snapshot();
    let key = Canonical::parse("urn:shared", None).unwrap();
    snapshot.systems.insert(
        key.clone(),
        PreparedSystem::prepare_for_r4(serde_json::from_value(source()).unwrap()).unwrap(),
    );
    let expected =
        serde_json::to_value(snapshot.expand(&root).unwrap().values().collect::<Vec<_>>()).unwrap();
    let prepared = Arc::new(snapshot.systems.remove(&key).unwrap());
    let shared = SharedSystems::from([(key, prepared.clone())]);
    let count = Arc::strong_count(&prepared);
    for _ in 0..4 {
        let result = snapshot.expand_with_shared(&root, &shared).unwrap();
        assert_eq!(
            serde_json::to_value(result.values().collect::<Vec<_>>()).unwrap(),
            expected
        );
        let plan = PreparedValueSet::prepare_with_shared(&snapshot, &root, &shared).unwrap();
        assert_eq!(
            plan.find("urn:shared", Some("1"), "ALPHA").unwrap().code,
            "Alpha"
        );
        assert!(plan.find("urn:shared", None, "Beta").is_none());
        assert_eq!(Arc::strong_count(&prepared), count);
    }
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..16 {
                    assert_eq!(
                        snapshot.expand_with_shared(&root, &shared).unwrap().len(),
                        2
                    );
                }
            });
        }
    });
    let index = prepared.code_index();
    assert!(index.complete());
    assert_eq!(index.resolve("ALPHA"), Some("Alpha"));
    assert_eq!(index.resolve("Beta"), Some("Beta"));
    assert_eq!(Arc::strong_count(&prepared), count);
    assert_eq!(prepared.concepts.len(), 3);
}
#[test]
fn shared_source_admission_preserves_strict_lifecycle_and_partial_coverage_errors() {
    for mode in [
        "valid",
        "undeclared",
        "unknown",
        "custom",
        "date",
        "conflict",
        "fragment",
    ] {
        let mut value = json!({"url":"urn:shared","version":"1","content":"complete","property":[{"code":"status","uri":"http://hl7.org/fhir/concept-properties#status","type":"code"}],"concept":[{"code":"Alpha","property":[{"code":"status","valueCode":"retired"}]}]});
        match mode {
            "undeclared" => {
                value.as_object_mut().unwrap().remove("property");
            }
            "unknown" => value["concept"][0]["property"][0]["valueCode"] = json!("unknown-state"),
            "custom" => value["property"][0]["uri"] = json!("urn:custom-status"),
            "date" => value["concept"][0]["property"]
                .as_array_mut()
                .unwrap()
                .push(json!({"code":"retirementDate","valueDateTime":"2026-01-01"})),
            "conflict" => value["concept"][0]["property"]
                .as_array_mut()
                .unwrap()
                .push(json!({"code":"inactive","valueBoolean":false})),
            "fragment" => value["content"] = json!("fragment"),
            _ => {}
        }
        let strict = PreparedSystem::prepare_for_r4(serde_json::from_value(value.clone()).unwrap())
            .map(|_| ());
        let lookup =
            PreparedSystem::prepare_for_r4_source(serde_json::from_value(value).unwrap()).unwrap();
        assert!(
            lookup.concept("Alpha").is_some(),
            "Lookup remains available for well-typed opaque lifecycle metadata"
        );
        assert_eq!(
            strict.as_ref().err().map(ToString::to_string),
            lookup.require_r4_composition().err().map(|e| e.to_string()),
            "{mode}"
        );
        let mut snapshot = Snapshot::default();
        let root = Canonical::parse("urn:set", None).unwrap();
        snapshot.value_sets.insert(
            root.clone(),
            serde_json::from_value(json!({"include":[{"system":"urn:shared"}]})).unwrap(),
        );
        let shared = SharedSystems::from([(
            Canonical::parse("urn:shared", None).unwrap(),
            Arc::new(lookup),
        )]);
        assert_eq!(
            snapshot.expand_with_shared(&root, &shared).is_ok(),
            mode == "valid",
            "{mode}"
        );
    }
}
#[test]
fn shared_source_views_reject_duplicate_identity_and_preserve_grammar_projection_restrictions() {
    let (mut snapshot, root) = snapshot();
    let key = Canonical::parse("urn:shared", None).unwrap();
    let prepared = Arc::new(
        PreparedSystem::prepare_for_r4(serde_json::from_value(source()).unwrap()).unwrap(),
    );
    for bad in [
        Canonical::parse("urn:other", None).unwrap(),
        Canonical::parse("urn:shared", Some("2")).unwrap(),
    ] {
        assert!(snapshot
            .expand_with_shared(&root, &SharedSystems::from([(bad, prepared.clone())]))
            .is_err());
    }
    snapshot.systems.insert(
        key.clone(),
        PreparedSystem::prepare_for_r4(serde_json::from_value(source()).unwrap()).unwrap(),
    );
    assert!(snapshot
        .expand_with_shared(&root, &SharedSystems::from([(key, prepared)]))
        .is_err());
    let mut snapshot = Snapshot::default();
    let units = Canonical::parse(ucum::SYSTEM, None).unwrap();
    let prepared=ucum::UcumProvider::prepare_explicit(serde_json::from_value(json!({"url":ucum::SYSTEM,"version":ucum::VERSION,"content":"fragment","concept":[{"code":"mg"}]})).unwrap()).unwrap();
    let shared = SharedSystems::from([(units, Arc::new(prepared))]);
    snapshot.value_sets.insert(
        root.clone(),
        serde_json::from_value(
            json!({"include":[{"system":ucum::SYSTEM,"concept":[{"code":"mg"}]}]}),
        )
        .unwrap(),
    );
    assert_eq!(
        snapshot.expand_with_shared(&root, &shared).unwrap().len(),
        1
    );
    snapshot.value_sets.insert(
        root.clone(),
        serde_json::from_value(json!({"include":[{"system":ucum::SYSTEM}]})).unwrap(),
    );
    assert!(snapshot.expand_with_shared(&root, &shared).is_err());
    assert!(!shared.values().next().unwrap().code_index().complete());
}
#[test]
fn shared_source_views_keep_aggregate_resource_and_concept_admission() {
    let mut snapshot = Snapshot::default();
    let root = Canonical::parse("urn:set", None).unwrap();
    snapshot.value_sets.insert(
        root.clone(),
        serde_json::from_value(json!({"include":[{"system":"urn:shared"}]})).unwrap(),
    );
    let mut shared = SharedSystems::new();
    for n in 0..MAX_RESOURCES {
        let url = format!("urn:limit:{n}");
        let source = json!({"url":url,"version":"1","content":"complete","concept":[{"code":"A"}]});
        shared.insert(
            Canonical::parse(&url, None).unwrap(),
            Arc::new(
                PreparedSystem::prepare_for_r4(serde_json::from_value(source).unwrap()).unwrap(),
            ),
        );
    }
    assert!(snapshot
        .expand_with_shared(&root, &shared)
        .unwrap_err()
        .to_string()
        .contains("resource limit"));
    let definition = CodeSystem {
        url: "urn:shared".into(),
        version: Some("1".into()),
        content: "complete".into(),
        case_sensitive: Some(true),
        hierarchy_meaning: None,
        property: vec![],
        concept: (0..50_001)
            .map(|n| Concept {
                code: format!("C{n}"),
                display: None,
                definition: None,
                designation: vec![],
                property: vec![],
                concept: vec![],
            })
            .collect(),
    };
    let system = Arc::new(PreparedSystem::prepare_for_r4(definition).unwrap());
    let shared = SharedSystems::from([
        (
            Canonical::parse("urn:shared", None).unwrap(),
            system.clone(),
        ),
        (Canonical::parse("urn:shared", Some("1")).unwrap(), system),
    ]);
    assert!(snapshot
        .expand_with_shared(&root, &shared)
        .unwrap_err()
        .to_string()
        .contains("concept limit"));
}
