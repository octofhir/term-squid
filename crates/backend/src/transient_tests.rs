//! Temporary terminology definitions must stay isolated from storage and retained plans.
use super::*;

fn context(parameters: Vec<Value>, resources: &[Value]) -> Value {
    let mut parameters = parameters;
    parameters.extend(
        resources
            .iter()
            .map(|resource| json!({"name":"tx-resource","resource":resource})),
    );
    json!({"resourceType":"Parameters","parameter":parameters})
}
fn definitions(display: &str) -> Vec<Value> {
    vec![
        json!({"resourceType":"CodeSystem","url":"urn:temporary:codes","version":"1","status":"active","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[{"code":"Root","concept":[{"code":"Alpha","display":display,"designation":[{"language":"ru","value":"Альфа"}]},{"code":"Outside"}]}]}),
        json!({"resourceType":"ValueSet","url":"urn:temporary:base","version":"1","status":"active","compose":{"include":[{"system":"urn:temporary:codes","version":"1","concept":[{"code":"Alpha"}]}]}}),
        json!({"resourceType":"ValueSet","url":"urn:temporary:set","version":"1","status":"active","compose":{"include":[{"valueSet":["urn:temporary:base|1"]}]}}),
        json!({"resourceType":"ConceptMap","url":"urn:temporary:map","version":"1","status":"active","group":[{"source":"urn:temporary:codes","sourceVersion":"1","target":"urn:temporary:target","element":[{"code":"Alpha","target":[{"code":"target","display":display,"equivalence":"equal"}]}]}]}),
    ]
}
async fn finish(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn all_six_operations_resolve_temporary_dependencies_without_persistence_or_retention() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    for cache in [true, false] {
        let store = Arc::new(if cache {
            PostgresStore::new(pool.clone())
        } else {
            PostgresStore::without_prepared_cache(pool.clone())
        });
        let app = api::create_router(store.clone());
        let resources = definitions("Temporary");
        for (operation, parameters, name, field, expected) in [
            (
                "CodeSystem/$lookup",
                vec![
                    json!({"name":"system","valueUri":"urn:temporary:codes"}),
                    json!({"name":"code","valueCode":"alpha"}),
                ],
                "display",
                "valueString",
                json!("Temporary"),
            ),
            (
                "CodeSystem/$validate-code",
                vec![
                    json!({"name":"url","valueUri":"urn:temporary:codes"}),
                    json!({"name":"code","valueCode":"alpha"}),
                ],
                "result",
                "valueBoolean",
                json!(true),
            ),
            (
                "CodeSystem/$subsumes",
                vec![
                    json!({"name":"system","valueUri":"urn:temporary:codes"}),
                    json!({"name":"codeA","valueCode":"Root"}),
                    json!({"name":"codeB","valueCode":"alpha"}),
                ],
                "outcome",
                "valueCode",
                json!("subsumes"),
            ),
            (
                "ValueSet/$validate-code",
                vec![
                    json!({"name":"url","valueUri":"urn:temporary:set"}),
                    json!({"name":"coding","valueCoding":{"system":"urn:temporary:codes","code":"alpha"}}),
                ],
                "result",
                "valueBoolean",
                json!(true),
            ),
            (
                "ConceptMap/$translate",
                vec![
                    json!({"name":"url","valueUri":"urn:temporary:map"}),
                    json!({"name":"system","valueUri":"urn:temporary:codes"}),
                    json!({"name":"code","valueCode":"Alpha"}),
                ],
                "result",
                "valueBoolean",
                json!(true),
            ),
        ] {
            let (status, body) = request(
                &app,
                "POST",
                &format!("/r4/{operation}"),
                context(parameters, &resources),
            )
            .await;
            assert_eq!(status, 200, "{operation}: {body}");
            assert_eq!(
                parameter(&body, name)[field],
                expected,
                "{operation}: {body}"
            );
        }
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            context(
                vec![json!({"name":"url","valueUri":"urn:temporary:set"})],
                &resources,
            ),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 1);
        assert_eq!(body["expansion"]["contains"][0]["display"], "Temporary");
        let (status, body) = request(&app,"POST","/r4/ValueSet/$validate-code",context(vec![json!({"name":"url","valueUri":"urn:temporary:set"}),json!({"name":"coding","valueCoding":{"system":"urn:temporary:codes","code":"missing"}})],&resources)).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], false);
        assert_eq!(
            parameter(&body, "issues")["resource"]["issue"]
                .as_array()
                .unwrap()
                .len(),
            2,
            "{body}"
        );
        let (status, body) = request(&app,"POST","/r4/ValueSet/$validate-code",context(vec![json!({"name":"url","valueUri":"urn:temporary:set"}),json!({"name":"coding","valueCoding":{"system":"urn:temporary:codes","code":"Alpha","display":"Альфа"}}),json!({"name":"displayLanguage","valueCode":"ru"})],&resources)).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
        assert_eq!(store.reusable_cache_usage().await, [(0, 0); 4]);
        for path in [
            "/r4/CodeSystem/$lookup?system=urn:temporary:codes&code=Alpha",
            "/r4/ValueSet/$expand?url=urn:temporary:set",
            "/r4/ConceptMap/$translate?url=urn:temporary:map&system=urn:temporary:codes&code=Alpha",
        ] {
            assert_eq!(
                request(&app, "GET", path, Value::Null).await.0,
                404,
                "{path}"
            );
        }
    }
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concept_maps)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    finish(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn temporary_overrides_do_not_poison_published_caches_or_instance_identity() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut published = definitions("Published");
    let system_id = insert_system(&pool, published[0].clone()).await;
    let set_id = insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:temporary:codes","version":"1"}]}),
    )
    .await;
    published[3]["url"] = json!("urn:map");
    let map_id = Uuid::new_v4();
    sqlx::query("INSERT INTO concept_maps(id,url,version,status,content) VALUES($1,'urn:map','1','active',$2)").bind(map_id).bind(sqlx::types::Json(&published[3])).execute(&pool).await.unwrap();
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    store
        .resolve_concept("urn:temporary:codes", Some("1"), "Alpha")
        .await
        .unwrap();
    store
        .resolve_subsumption("urn:temporary:codes", Some("1"), "Root", "Alpha")
        .await
        .unwrap();
    store.expand_compose("urn:set", Some("1")).await.unwrap();
    store
        .prepare_concept_map("urn:map", Some("1"), None)
        .await
        .unwrap();
    let retained = store.reusable_cache_usage().await;
    assert!(retained.iter().all(|(count, _)| *count > 0));
    let mut resources = definitions("Override");
    resources[0]["id"] = json!(system_id);
    resources[3]["url"] = json!("urn:map");
    resources[3]["id"] = json!(map_id);
    for path in [
        "/r4/ValueSet/$expand",
        &format!("/r4/ValueSet/{set_id}/$expand"),
    ] {
        let parameters = if path.contains(&set_id.to_string()) {
            vec![]
        } else {
            vec![json!({"name":"url","valueUri":"urn:set"})]
        };
        let (status, body) = request(&app, "POST", path, context(parameters, &resources)).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            body["expansion"]["contains"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["code"] == "Alpha")
                .unwrap()["display"],
            "Override"
        );
    }
    let (status, body) = request(
        &app,
        "POST",
        &format!("/r4/CodeSystem/{system_id}/$lookup"),
        context(vec![json!({"name":"code","valueCode":"Alpha"})], &resources),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        parameter(&body, "display")["valueString"],
        "Published",
        "Client resource.id must not replace an instance"
    );
    let parameters = vec![
        json!({"name":"system","valueUri":"urn:temporary:codes"}),
        json!({"name":"code","valueCode":"Alpha"}),
    ];
    let (status, body) = request(
        &app,
        "POST",
        &format!("/r4/ConceptMap/{map_id}/$translate"),
        context(parameters, &resources),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        parameter(&body, "match")["part"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "concept")
            .unwrap()["valueCoding"]["display"],
        "Published"
    );
    assert_eq!(store.reusable_cache_usage().await, retained);
    let (status, body) =
        request(&app, "GET", "/r4/ValueSet/$expand?url=urn:set", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["expansion"]["contains"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["code"] == "Alpha")
            .unwrap()["display"],
        "Published"
    );
    let display = store
        .resolve_concept("urn:temporary:codes", Some("1"), "Alpha")
        .await
        .unwrap()
        .unwrap()
        .display;
    assert_eq!(display.as_deref(), Some("Published"));
    drop(app);
    drop(store);
    finish(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concurrent_contexts_with_the_same_canonical_remain_isolated() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..12 {
        let app = app.clone();
        tasks.spawn(async move {
            let label = format!("Request {index}");
            for _ in 0..3 {
                let (status, body) = request(
                    &app,
                    "POST",
                    "/r4/ValueSet/$expand",
                    context(
                        vec![json!({"name":"url","valueUri":"urn:temporary:set"})],
                        &definitions(&label),
                    ),
                )
                .await;
                assert_eq!(status, 200, "{body}");
                assert_eq!(body["expansion"]["contains"][0]["display"], label);
            }
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_eq!(store.reusable_cache_usage().await, [(0, 0); 4]);
    assert_eq!(
        request(
            &app,
            "GET",
            "/r4/ValueSet/$expand?url=urn:temporary:set",
            Value::Null
        )
        .await
        .0,
        404
    );
    drop(app);
    drop(store);
    finish(admin, pool, schema).await;
}

fn prepare(
    resources: Vec<Value>,
    date: Option<&str>,
) -> Result<store::transient::Resources, error::AppError> {
    store::transient::Resources::prepare(
        resources,
        date,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

#[test]
fn temporary_resource_identity_versions_bounds_and_authoritative_provider_guards() {
    use store::transient::Kind;
    let base = definitions("One")[0].clone();
    let mut newer = base.clone();
    newer["version"] = json!("2");
    newer["concept"][0]["concept"][0]["display"] = json!("Two");
    let resources = prepare(vec![base.clone(), newer.clone(), newer.clone()], None).unwrap();
    assert_eq!(
        resources
            .select(Kind::System, "urn:temporary:codes", None, None, None)
            .unwrap()
            .unwrap()
            .concept("Alpha")
            .unwrap()
            .display
            .as_deref(),
        Some("Two")
    );
    assert_eq!(
        resources
            .select(Kind::System, "urn:temporary:codes|1", None, None, None)
            .unwrap()
            .unwrap()
            .concept("Alpha")
            .unwrap()
            .display
            .as_deref(),
        Some("One")
    );
    assert!(resources
        .select(Kind::System, "urn:temporary:codes|1", Some("2"), None, None)
        .is_err());
    assert!(resources
        .select(
            Kind::System,
            "urn:temporary:codes",
            None,
            Some(Uuid::new_v4()),
            None
        )
        .unwrap()
        .is_none());
    let mut conflict = base.clone();
    conflict["concept"][0]["display"] = json!("Conflicting");
    assert!(prepare(vec![base.clone(), conflict], None).is_err());
    assert!(prepare(vec![base.clone(); 33], None).is_err());
    for url in [
        "http://unitsofmeasure.org",
        "http://loinc.org",
        "http://snomed.info/sct",
        "http://snomed.info/sct/900000000000207008",
    ] {
        let mut resource = base.clone();
        resource["url"] = json!(url);
        assert!(prepare(vec![resource], None).is_err(), "{url}");
    }
    for resource in [
        json!({"resourceType":"Patient","url":"urn:patient"}),
        json!({"resourceType":"CodeSystem","url":"urn:x|1","content":"complete"}),
        json!({"resourceType":"ValueSet","url":"urn:x","version":7}),
    ] {
        assert!(prepare(vec![resource], None).is_err());
    }
    let mut oversized = base.clone();
    oversized["description"] = json!("x".repeat(256 * 1024));
    assert!(prepare(vec![oversized], None).is_err());
    let mut incomplete = base.clone();
    incomplete["content"] = json!("not-present");
    let resources = prepare(vec![incomplete], None).unwrap();
    let entry = resources
        .select(Kind::System, "urn:temporary:codes", None, None, None)
        .unwrap()
        .unwrap();
    assert_eq!(
        entry.concept("Alpha").unwrap().code.as_deref(),
        Some("Alpha")
    );
    assert!(entry.subsumption("Root", "Alpha").is_err());
    assert!(matches!(
        store::transient::Resources::prepare(
            vec![base],
            None,
            &std::sync::atomic::AtomicBool::new(true)
        ),
        Err(error::AppError::Overloaded)
    ));
}

#[test]
fn temporary_supplements_select_one_edition_and_honor_the_historical_cutoff() {
    use store::transient::Kind;
    let mut base = definitions("One")[0].clone();
    base["date"] = json!("2020-01-01");
    let mut supplement = json!({"resourceType":"CodeSystem","url":"urn:temporary:supplement","version":"1","status":"active","date":"2021-01-01","content":"supplement","supplements":"urn:temporary:codes|1","concept":[{"code":"Alpha","designation":[{"language":"de","value":"Erste"}]}]});
    let mut second = supplement.clone();
    second["version"] = json!("2");
    second["date"] = json!("2022-01-01");
    second["concept"][0]["designation"][0]["value"] = json!("Zweite");
    for (cutoff, expected) in [
        (None, Some("Zweite")),
        (Some("2021-06-01"), Some("Erste")),
        (Some("2020-06-01"), None),
    ] {
        let resources = prepare(
            vec![base.clone(), supplement.clone(), second.clone()],
            cutoff,
        )
        .unwrap();
        let entry = resources
            .select(Kind::System, "urn:temporary:codes", None, None, cutoff)
            .unwrap()
            .unwrap();
        let designations = entry
            .lookup(&store::LookupRequest {
                url: "urn:temporary:codes",
                version: Some("1"),
                id: None,
                code: "Alpha",
                date: cutoff,
                language: None,
                relationships: false,
                metadata: true,
                require_semantic_types: false,
                require_attributes: false,
                require_primary_links: false,
            })
            .unwrap()
            .designations;
        let german = designations
            .iter()
            .filter(|d| d["language"] == "de")
            .collect::<Vec<_>>();
        assert_eq!(german.len(), usize::from(expected.is_some()));
        if let Some(expected) = expected {
            assert_eq!(german[0]["value"], expected);
        }
    }
    assert!(prepare(vec![supplement.clone()], None).is_err());
    supplement["concept"][0]["code"] = json!("Invented");
    assert!(prepare(vec![base, supplement], None).is_err());
}

#[tokio::test]
async fn temporary_scopes_restore_on_drop_and_are_not_inherited_by_spawned_tasks() {
    let context = Arc::new(prepare(definitions("Scoped"), None).unwrap());
    assert!(!store::transient::active());
    store::transient::scope(context.clone(), async {
        assert!(store::transient::active());
        assert!(tokio::spawn(async { !store::transient::active() })
            .await
            .unwrap());
    })
    .await;
    assert!(!store::transient::active());
    let mut future = Box::pin(store::transient::scope(context, async {
        assert!(store::transient::active());
        std::future::pending::<()>().await;
    }));
    tokio::select! { ()=&mut future=>unreachable!(), ()=tokio::task::yield_now()=>{} }
    drop(future);
    assert!(!store::transient::active());
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn temporary_coding_versions_discovery_chains_and_scope_membership_use_one_context() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut resources = definitions("First");
    let mut second = resources[0].clone();
    second["version"] = json!("2");
    second["concept"] = json!([{"code":"Different","display":"Second"}]);
    resources.push(second);
    for input in [
        json!({"name":"coding","valueCoding":{"system":"urn:temporary:codes","version":"1","code":"Alpha"}}),
        json!({"name":"codeableConcept","valueCodeableConcept":{"coding":[{"system":"urn:temporary:codes","version":"2","code":"missing"},{"system":"urn:temporary:codes","version":"1","code":"Alpha"}]}}),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$validate-code",
            context(
                vec![
                    json!({"name":"url","valueUri":"urn:temporary:codes"}),
                    input,
                ],
                &resources,
            ),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
        assert_eq!(parameter(&body, "version")["valueString"], "1");
    }
    resources[3]["url"] = json!("urn:temporary:next");
    resources.push(json!({"resourceType":"ConceptMap","url":"urn:temporary:chain","version":"1","group":[{"source":"urn:temporary:codes","target":"urn:temporary:target","unmapped":{"mode":"other-map","url":"urn:temporary:next|1"}}]}));
    for parameters in [
        vec![
            json!({"name":"system","valueUri":"urn:temporary:codes"}),
            json!({"name":"code","valueCode":"Alpha"}),
        ],
        vec![
            json!({"name":"url","valueUri":"urn:temporary:chain"}),
            json!({"name":"system","valueUri":"urn:temporary:codes"}),
            json!({"name":"code","valueCode":"Alpha"}),
            json!({"name":"source","valueUri":"urn:temporary:set|1"}),
        ],
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ConceptMap/$translate",
            context(parameters, &resources),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
        assert!(
            body["parameter"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|p| p["name"] == "match")
                .any(
                    |p| p["part"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|part| part["name"] == "concept"
                            && part["valueCoding"]["code"] == "target")
                ),
            "{body}"
        );
    }
    resources.push(json!({"resourceType":"ConceptMap","url":"urn:temporary:fixed","group":[{"source":"urn:temporary:codes","sourceVersion":"1","target":"urn:temporary:target","unmapped":{"mode":"fixed","code":"fixed"}}]}));
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ConceptMap/$translate",
        context(
            vec![
                json!({"name":"url","valueUri":"urn:temporary:fixed"}),
                json!({"name":"system","valueUri":"urn:temporary:target"}),
                json!({"name":"code","valueCode":"fixed"}),
                json!({"name":"reverse","valueBoolean":true}),
            ],
            &resources,
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
    let matches = body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "match")
        .count();
    assert_eq!(matches, 3, "{body}");
    drop(app);
    finish(admin, pool, schema).await;
}

#[tokio::test]
async fn malformed_temporary_resources_fail_before_any_database_connection() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let base = definitions("Valid")[0].clone();
    let mut conflict = base.clone();
    conflict["concept"] = json!([{"code":"different"}]);
    for resources in [
        vec![json!({"resourceType":"Patient","url":"urn:patient"})],
        vec![base.clone(), conflict],
        vec![base.clone(); 33],
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$lookup",
            context(
                vec![
                    json!({"name":"system","valueUri":"urn:temporary:codes"}),
                    json!({"name":"code","valueCode":"Alpha"}),
                ],
                &resources,
            ),
        )
        .await;
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    for invalid in [
        json!({"name":"tx-resource","valueString":"urn:codes"}),
        json!({"name":"tx-resource","resource":base,"part":[]}),
        json!({"name":"tx-resource","resource":base,"valueString":"mixed"}),
        json!({"name":"tx-resource","resource":base,"valueUnknown":"hidden"}),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$lookup",
            json!({"resourceType":"Parameters","parameter":[invalid]}),
        )
        .await;
        assert_eq!(status, 400, "{body}");
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn expansion_provenance_keeps_actual_editions_even_when_no_members_are_returned() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut resources = definitions("First");
    resources.push(json!({"resourceType":"CodeSystem","url":"urn:temporary:supplement","version":"3","content":"supplement","status":"active","supplements":"urn:temporary:codes|1","concept":[{"code":"Alpha","designation":[{"language":"de","value":"Erste"}]}]}));
    for count in [0, 1] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            context(
                vec![
                    json!({"name":"url","valueUri":"urn:temporary:set"}),
                    json!({"name":"count","valueInteger":count}),
                ],
                &resources,
            ),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 1);
        assert_eq!(
            body["expansion"]["contains"].as_array().unwrap().len(),
            count as usize
        );
        let actual = body["expansion"]["parameter"].as_array().unwrap();
        for (name, identity) in [
            ("used-codesystem", "urn:temporary:codes|1"),
            ("used-valueset", "urn:temporary:base|1"),
            ("used-supplement", "urn:temporary:supplement|3"),
        ] {
            assert!(
                actual.contains(&json!({"name":name,"valueUri":identity})),
                "{body}"
            );
        }
        assert!(!actual.iter().any(|p| p["name"] == "offset"));
        assert!(body["expansion"].get("offset").is_none());
    }
    let mut second = resources[0].clone();
    second["version"] = json!("2");
    second["concept"][0]["concept"][0]["display"] = json!("Second");
    resources.push(second);
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        context(
            vec![
                json!({"name":"url","valueUri":"urn:temporary:set"}),
                json!({"name":"force-system-version","valueCanonical":"urn:temporary:codes|2"}),
                json!({"name":"offset","valueInteger":0}),
            ],
            &resources,
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["contains"][0]["version"], "2");
    assert_eq!(body["expansion"]["contains"][0]["display"], "Second");
    let actual = body["expansion"]["parameter"].as_array().unwrap();
    assert!(
        actual.contains(&json!({"name":"used-codesystem","valueUri":"urn:temporary:codes|2"})),
        "{body}"
    );
    assert!(
        !actual.iter().any(|p| p["name"] == "used-supplement"),
        "A version-1 supplement cannot be reported as used for version 2: {body}"
    );
    assert_eq!(body["expansion"]["offset"], 0);
    assert!(!actual.iter().any(|p|p["name"]=="count"),"A default count that does not truncate the result is not an effective selection constraint: {body}");
    drop(app);
    finish(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn shared_temporary_preparations_keep_strict_compose_policy_and_complete_negative_knowledge()
{
    let (admin, pool, schema) = hierarchy_test_database().await;
    for retained in [true, false] {
        let app = api::create_router(Arc::new(if retained {
            PostgresStore::new(pool.clone())
        } else {
            PostgresStore::without_prepared_cache(pool.clone())
        }));
        for mode in [
            "valid",
            "undeclared-status",
            "unknown-status",
            "retirement-date",
            "fragment",
        ] {
            let mut source = json!({"resourceType":"CodeSystem","url":"urn:shared-context","version":"1","status":"active","content":"complete","caseSensitive":false,"concept":[{"code":"Alpha","display":"Alpha"}]});
            match mode {
                "undeclared-status" => {
                    source["concept"][0]["property"] =
                        json!([{"code":"status","valueCode":"retired"}])
                }
                "unknown-status" => {
                    source["property"] = json!([{"code":"life","type":"code","uri":"http://hl7.org/fhir/concept-properties#status"}]);
                    source["concept"][0]["property"] =
                        json!([{"code":"life","valueCode":"future-state"}]);
                }
                "retirement-date" => {
                    source["concept"][0]["property"] =
                        json!([{"code":"retirementDate","valueDateTime":"2026-01-01"}])
                }
                "fragment" => source["content"] = json!("fragment"),
                _ => {}
            }
            let (status, body) = request(
                &app,
                "POST",
                "/r4/CodeSystem/$lookup",
                context(
                    vec![
                        json!({"name":"system","valueUri":"urn:shared-context"}),
                        json!({"name":"code","valueCode":"alpha"}),
                    ],
                    std::slice::from_ref(&source),
                ),
            )
            .await;
            assert_eq!(status, 200, "{mode}: {body}");
            assert_eq!(parameter(&body, "display")["valueString"], "Alpha");
            let definition = json!({"resourceType":"ValueSet","status":"active","compose":{"include":[{"system":"urn:shared-context"}]}});
            let (status, body) = request(
                &app,
                "POST",
                "/r4/ValueSet/$expand",
                context(
                    vec![json!({"name":"valueSet","resource":definition})],
                    std::slice::from_ref(&source),
                ),
            )
            .await;
            assert_eq!(
                status,
                if mode == "valid" { 200 } else { 422 },
                "{mode}: {body}"
            );
            if mode == "valid" {
                assert_eq!(body["expansion"]["total"], 1);
                let (status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",context(vec![json!({"name":"valueSet","resource":definition}),json!({"name":"coding","valueCoding":{"system":"urn:shared-context","code":"missing"}})],std::slice::from_ref(&source))).await;
                assert_eq!(status, 200, "{body}");
                assert_eq!(parameter(&body, "result")["valueBoolean"], false);
                assert!(
                    body.to_string().contains("invalid-code"),
                    "Complete shared knowledge still distinguishes an unknown code: {body}"
                );
            }
        }
        let (status, body) = request(
            &app,
            "GET",
            "/r4/CodeSystem/$lookup?system=urn:shared-context&code=Alpha",
            Value::Null,
        )
        .await;
        assert_eq!(
            status, 404,
            "No shared preparation can escape its request: {body}"
        );
    }
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0));
    finish(admin, pool, schema).await;
}

#[test]
fn exact_duplicate_contexts_keep_their_last_request_position_without_changing_date_precedence() {
    use store::transient::Kind;
    let examples = [
        (Kind::System, definitions("First")[0].clone()),
        (Kind::Set, definitions("First")[1].clone()),
        (Kind::Map, definitions("First")[3].clone()),
    ];
    for (kind, mut first) in examples {
        first["date"] = json!("2025-01-01");
        let mut second = first.clone();
        second["version"] = json!("2");
        let url = first["url"].as_str().unwrap();
        let resources = prepare(vec![first.clone(), second.clone(), first.clone()], None).unwrap();
        for date in [None, Some("2026")] {
            let selected = resources
                .select(kind, url, None, None, date)
                .unwrap()
                .unwrap();
            assert_eq!(
                selected.resource["version"], "1",
                "Exact deduplication must preserve the last supplied edition position"
            );
        }
        second["date"] = json!("2025-01-02");
        let resources = prepare(vec![first.clone(), second, first.clone()], None).unwrap();
        assert_eq!(
            resources
                .select(kind, url, None, None, None)
                .unwrap()
                .unwrap()
                .resource["version"],
            "1"
        );
        assert_eq!(
            resources
                .select(kind, url, None, None, Some("2026"))
                .unwrap()
                .unwrap()
                .resource["version"],
            "2",
            "Historical selection still prioritizes publication date"
        );
    }
    let base = definitions("Base")[0].clone();
    let first = json!({"resourceType":"CodeSystem","url":"urn:temporary:supplement-duplicate","version":"1","status":"active","content":"supplement","supplements":"urn:temporary:codes|1","concept":[{"code":"Alpha","designation":[{"language":"en","value":"First supplement"}]}]});
    let mut second = first.clone();
    second["version"] = json!("2");
    second["concept"][0]["designation"][0]["value"] = json!("Second supplement");
    let resources = prepare(vec![base, first.clone(), second, first], None).unwrap();
    let selected = resources
        .select(Kind::System, "urn:temporary:codes", None, None, None)
        .unwrap()
        .unwrap();
    let labels = selected.designations("Alpha").unwrap();
    assert!(labels.iter().any(|d| d["value"] == "First supplement"));
    assert!(!labels.iter().any(|d| d["value"] == "Second supplement"));
}
