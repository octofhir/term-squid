use term_squid_core::ucum::UcumProvider;
fn main() {
    assert!(UcumProvider::lookup("mg/dL", Some("2.2"))
        .unwrap()
        .is_some());
    assert!(UcumProvider::lookup("not-a-unit", None).unwrap().is_none());
    assert!(UcumProvider::comparable("mg/dL", "g/L").unwrap());
    assert!(!UcumProvider::comparable("m", "s").unwrap());
    assert!(UcumProvider::lookup("m", Some("missing")).is_err());
    assert_eq!(UcumProvider::convert(1000.0, "mg", "g").unwrap(), 1.0);
    assert_eq!(UcumProvider::canonical("km").unwrap().unit, "m");
    assert!(UcumProvider::convert(f64::NAN, "m", "cm").is_err());
    finite_r4_filters();
    finite_ucum();
    rxnorm_filters();
    snomed_ecl();
    loinc_parts_and_snomed_refsets();
}

fn loinc_parts_and_snomed_refsets() {
    use term_squid_core::{
        loinc::{PrimaryAxis, PrimaryFilter},
        snomed::{ecl::Ecl, NativeFilter},
        Filter,
    };
    let axis = PrimaryAxis::Time;
    assert_eq!(axis.part_type(), "TIME");
    assert_eq!(axis.code(), "TIME_ASPCT");
    assert_eq!(axis.uri(), "http://loinc.org/property/TIME_ASPCT");
    let parts = PrimaryFilter::parse(&Filter {
        property: "COMPONENT".into(),
        op: "in".into(),
        value: "lp100-1,LP200-2".into(),
    })
    .unwrap();
    assert!(matches!(
        parts,
        PrimaryFilter::Parts {
            axis: PrimaryAxis::Component,
            ..
        }
    ));
    assert!(!parts.scopes_primary_table());
    let refset = NativeFilter::parse(&Filter {
        property: "concept".into(),
        op: "in".into(),
        value: "700001".into(),
    })
    .unwrap();
    assert_eq!(refset, NativeFilter::Refset("700001".into()));
    let ecl = Ecl::parse("^700001 AND <<100001").unwrap();
    assert_eq!(ecl.refsets(), std::collections::BTreeSet::from(["700001"]));
    assert_eq!(ecl.hierarchies().len(), 1);
}

fn rxnorm_filters() {
    use term_squid_core::{
        rxnorm::{Filter as RxNormFilter, SemanticTypeField},
        Filter,
    };
    let filter = |property: &str, op: &str, value: &str| Filter {
        property: property.into(),
        op: op.into(),
        value: value.into(),
    };
    assert_eq!(
        RxNormFilter::parse(&filter("STY", "in", "TUI:T109,T121")).unwrap(),
        RxNormFilter::SemanticTypes {
            field: SemanticTypeField::Tui,
            values: vec!["T109".into(), "T121".into()],
        }
    );
    assert_eq!(
        RxNormFilter::parse(&filter("NDC", "=", "00012345678")).unwrap(),
        RxNormFilter::NormalizedNdc(vec!["00012345678".into()])
    );
    assert!(RxNormFilter::parse(&filter("NDC", "=", "1234567890")).is_err());
    assert!(RxNormFilter::parse(&filter("STY", "in", "TUI:T109,STN:A1")).is_err());
    let relation = RxNormFilter::parse(&filter("has_ingredient", "in", "CUI:111,222")).unwrap();
    assert!(matches!(
        relation,
        RxNormFilter::Relationship {
            target: term_squid_core::rxnorm::RelationshipTarget::Cui,
            ..
        }
    ));
    assert!(!relation.scopes_active());
}

fn snomed_ecl() {
    use term_squid_core::snomed::{ecl::Ecl, NativeFilter};
    let ecl = Ecl::parse("(<< 100001 MINUS << 100002) OR <! 100003").unwrap();
    assert_eq!(ecl.anchors().len(), 3);
    assert_eq!(ecl.hierarchies().len(), 3);
    assert!(Ecl::parse("100001 OR 100002 AND 100003").is_err());
    assert!(Ecl::parse("<< 100001 : 100002 = 100003").is_err());
    let filter = term_squid_core::Filter {
        property: "constraint".into(),
        op: "=".into(),
        value: ecl.source().into(),
    };
    assert!(matches!(
        NativeFilter::parse(&filter).unwrap(),
        NativeFilter::Constraint(_)
    ));
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
