use term_squid_core::ucum::UcumProvider;
fn main() {
    assert!(UcumProvider::lookup("mg/dL", Some("2.2"))
        .unwrap()
        .is_some());
    assert!(UcumProvider::lookup("not-a-unit", None).unwrap().is_none());
    assert!(UcumProvider::comparable("mg/dL", "g/L").unwrap());
    assert!(!UcumProvider::comparable("m", "s").unwrap());
    assert!(UcumProvider::lookup("m", Some("missing")).is_err());
    finite_r4_filters();
    finite_ucum();
}

fn finite_r4_filters() {
    use term_squid_core::{
        Canonical, CodeSystem, Compose, Concept, Filter, PreparedSystem, PreparedValueSet,
        Selection, Snapshot,
    };
    let system = Canonical::parse("urn:standalone:codes", None).unwrap();
    let set = Canonical::parse("urn:standalone:set", None).unwrap();
    let definition = CodeSystem {
        url: system.url.clone(),
        version: Some("1".into()),
        content: "complete".into(),
        case_sensitive: Some(false),
        hierarchy_meaning: None,
        property: vec![],
        concept: ["Alpha", "Beta"]
            .into_iter()
            .map(|code| Concept {
                code: code.into(),
                display: Some(code.into()),
                definition: None,
                designation: vec![],
                property: vec![],
                concept: vec![],
            })
            .collect(),
    };
    let mut snapshot = Snapshot::default();
    snapshot.systems.insert(
        system.clone(),
        PreparedSystem::prepare_for_r4(definition).unwrap(),
    );
    snapshot.value_sets.insert(
        set.clone(),
        Compose {
            inactive: None,
            locked_date: None,
            exclude: vec![],
            include: vec![Selection {
                system: Some(system.url.clone()),
                version: None,
                concept: vec![],
                value_set: vec![],
                filter: vec![Filter {
                    property: "code".into(),
                    op: "regex".into(),
                    value: "alpha".into(),
                }],
            }],
        },
    );
    let plan = PreparedValueSet::prepare(&snapshot, &set).unwrap();
    assert_eq!(plan.values().count(), 1);
    assert_eq!(
        plan.find(&system.url, Some("1"), "ALPHA").unwrap().code,
        "Alpha"
    );
    assert!(plan.find(&system.url, None, "Beta").is_none());
    let shared = term_squid_core::SharedSystems::from([(
        system.clone(),
        std::sync::Arc::new(snapshot.systems.remove(&system).unwrap()),
    )]);
    let shared_plan = PreparedValueSet::prepare_with_shared(&snapshot, &set, &shared).unwrap();
    assert_eq!(shared_plan.values().count(), 1);
    assert_eq!(
        shared_plan
            .find(&system.url, Some("1"), "ALPHA")
            .unwrap()
            .code,
        "Alpha"
    );
    assert_eq!(
        std::sync::Arc::strong_count(shared.get(&system).unwrap()),
        1
    );
    let knowledge = std::sync::Arc::try_unwrap(shared.into_values().next().unwrap())
        .unwrap()
        .into_code_index();
    assert!(knowledge.complete());
    assert_eq!(knowledge.resolve("ALPHA"), Some("Alpha"));
    assert_eq!(knowledge.resolve("Beta"), Some("Beta"));
    assert_eq!(knowledge.resolve("Missing"), None);
}

fn finite_ucum() {
    use term_squid_core::{CodeSystem, Concept};
    let prepared = UcumProvider::prepare_explicit(CodeSystem {
        url: term_squid_core::ucum::SYSTEM.into(),
        version: Some(term_squid_core::ucum::VERSION.into()),
        content: "fragment".into(),
        case_sensitive: Some(true),
        hierarchy_meaning: None,
        property: vec![],
        concept: vec![Concept {
            code: "mg/dL".into(),
            display: None,
            definition: None,
            designation: vec![],
            property: vec![],
            concept: vec![],
        }],
    })
    .unwrap();
    assert!(prepared.concept("mg/dL").unwrap().display.is_some());
    let knowledge = prepared.into_code_index();
    assert!(!knowledge.complete());
    assert_eq!(knowledge.resolve("mg/dL"), Some("mg/dL"));
    assert_eq!(knowledge.resolve("kg"), None);
}
