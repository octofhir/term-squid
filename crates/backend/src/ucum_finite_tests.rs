//! A finite ValueSet can select real grammar-validated units without enumerating UCUM.
use super::*;
const UNITS: &str = "http://unitsofmeasure.org";
fn parameters(fields: Vec<Value>) -> Value {
    json!({"resourceType":"Parameters","parameter":fields})
}
async fn publish(pool: &PgPool, source: &Value) -> Uuid {
    let inspection = term_squid_ingestion::inspect(source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let result = term_squid_ingestion::import_resource(&mut tx, source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    result.id
}
async fn cleanup(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
fn child(version: &str, codes: &[&str]) -> Value {
    json!({"resourceType":"ValueSet","url":"urn:explicit-ucum:child","version":version,"status":"active","date":"2026-01-01","compose":{"include":[{"system":UNITS,"concept":codes.iter().map(|code|json!({"code":code})).collect::<Vec<_>>()}]}})
}
#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ucum_finite_membership_expansion_context_versions_and_activation_use_the_real_grammar() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut definition = child("1", &["mg", "mg/dL", "Cel", "mm[Hg]"]);
    definition["compose"]["exclude"] = json!([{"system":UNITS,"concept":[{"code":"Cel"}]}]);
    publish(&pool, &definition).await;
    publish(&pool,&json!({"resourceType":"CodeSystem","url":"urn:ordinary-units","version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"Ordinary"}]})).await;
    let root = json!({"resourceType":"ValueSet","url":"urn:explicit-ucum:root","version":"1","status":"active","date":"2026-01-01","compose":{"include":[{"valueSet":["urn:explicit-ucum:child"]},{"system":"urn:ordinary-units","concept":[{"code":"A"}]}]}});
    let root_id = publish(&pool, &root).await;
    let stores = [
        Arc::new(PostgresStore::new(pool.clone())),
        Arc::new(PostgresStore::without_prepared_cache(pool.clone())),
    ];
    for store in &stores {
        let app = api::create_router(store.clone());
        for path in [
            "/r4/ValueSet/$expand?url=urn:explicit-ucum:root".to_owned(),
            format!("/r4/ValueSet/{root_id}/$expand"),
        ] {
            let (status, body) = request(&app, "GET", &path, Value::Null).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(body["expansion"]["total"], 4);
            let units: Vec<_> = body["expansion"]["contains"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|c| c["system"] == UNITS)
                .collect();
            assert_eq!(units.len(), 3);
            assert!(units.iter().all(|c| c["version"] == "2.2"));
            assert_eq!(
                units.iter().find(|c| c["code"] == "mg").unwrap()["display"],
                "milligram"
            );
            assert!(
                body["expansion"]["parameter"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|p| p["name"] == "used-codesystem"
                        && p["valueUri"] == format!("{UNITS}|2.2"))
            );
        }
        for (code, valid) in [
            ("mg", true),
            ("mg/dL", true),
            ("Cel", false),
            ("kg", false),
            ("not-a-unit", false),
        ] {
            let mut query = form_urlencoded::Serializer::new(String::new());
            query
                .append_pair("url", "urn:explicit-ucum:root")
                .append_pair("system", UNITS)
                .append_pair("code", code);
            let (status, body) = request(
                &app,
                "GET",
                &format!("/r4/ValueSet/$validate-code?{}", query.finish()),
                Value::Null,
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
            let (status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",parameters(vec![json!({"name":"url","valueUri":"urn:explicit-ucum:root"}),json!({"name":"coding","valueCoding":{"system":UNITS,"version":"2.2","code":code}})])).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
            if code == "kg" {
                assert!(!body.to_string().contains("invalid-code"),"Partial projection knowledge must not claim a valid unlisted grammar code is unknown: {body}");
            }
        }
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![
                json!({"name":"valueSet","resource":root}),
                json!({"name":"count","valueInteger":2}),
                json!({"name":"offset","valueInteger":1}),
                json!({"name":"check-system-version","valueCanonical":format!("{UNITS}|2.2")}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 4);
        assert_eq!(body["expansion"]["contains"].as_array().unwrap().len(), 2);
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(vec![
                json!({"name":"url","valueUri":"urn:explicit-ucum:child"}),
                json!({"name":"code","valueCode":"mg"}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
        let temporary = child("temporary", &["kg"]);
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![
                json!({"name":"url","valueUri":"urn:explicit-ucum:root"}),
                json!({"name":"tx-resource","resource":temporary}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 2);
        let (status, body) = request(
            &app,
            "GET",
            "/r4/ValueSet/$expand?url=urn:explicit-ucum:root",
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            body["expansion"]["total"], 4,
            "Temporary codes cannot poison the stored plan"
        );
    }
    let held = stores[0]
        .expand_compose("urn:explicit-ucum:root", None)
        .await
        .unwrap();
    publish(&pool, &child("2", &["kg"])).await;
    for store in &stores {
        let next = store
            .expand_compose("urn:explicit-ucum:root", None)
            .await
            .unwrap();
        assert_eq!(next.members.values().count(), 2);
        assert!(next.members.find(UNITS, Some("2.2"), "kg").is_some());
        assert!(next.members.find(UNITS, None, "mg").is_none());
    }
    assert_eq!(held.members.values().count(), 4);
    assert!(held.members.find(UNITS, Some("2.2"), "mg").is_some());
    let source = child("3", &["Cel"]);
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    for store in &stores {
        assert!(store
            .expand_compose("urn:explicit-ucum:root", None)
            .await
            .unwrap()
            .members
            .find(UNITS, None, "kg")
            .is_some());
    }
    let synthetic: i64 = sqlx::query_scalar("SELECT count(*) FROM code_systems WHERE url=$1")
        .bind(UNITS)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        synthetic, 0,
        "Grammar projections must never be published as fake complete CodeSystems"
    );
    drop(stores);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ucum_finite_supplements_localization_dates_and_freshness_keep_projection_scope() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    publish(&pool, &child("1", &["mg"])).await;
    let stores = [
        Arc::new(PostgresStore::new(pool.clone())),
        Arc::new(PostgresStore::without_prepared_cache(pool.clone())),
    ];
    let mut supplement = json!({"resourceType":"CodeSystem","url":"urn:ucum-list:supplement","version":"1","date":"2025-01-01","status":"active","content":"supplement","supplements":format!("{UNITS}|2.2"),"concept":[{"code":"mg","designation":[{"language":"de","value":"Milligramm"}]},{"code":"kg","designation":[{"language":"de","value":"Kilogramm"}]}]});
    publish(&pool, &supplement).await;
    for store in &stores {
        let app = api::create_router(store.clone());
        let (status,body)=request(&app,"GET","/r4/ValueSet/$expand?url=urn:explicit-ucum:child&displayLanguage=de&includeDesignations=true&check-system-version=http://unitsofmeasure.org%7C2.2&date=2026",Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 1);
        assert_eq!(body["expansion"]["contains"][0]["display"], "Milligramm");
        let (status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",parameters(vec![json!({"name":"url","valueUri":"urn:explicit-ucum:child"}),json!({"name":"coding","valueCoding":{"system":UNITS,"code":"mg","display":"Milligramm"}}),json!({"name":"displayLanguage","valueCode":"de"})])).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
        // Both aliases share all explicitly requested grammar codes and incomplete knowledge.
        let inline = json!({"resourceType":"ValueSet","status":"active","compose":{"include":[{"system":UNITS,"concept":[{"code":"mg"}]},{"system":UNITS,"version":"2.2","concept":[{"code":"kg"}]}]}});
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![
                json!({"name":"valueSet","resource":inline}),
                json!({"name":"displayLanguage","valueCode":"de"}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 2);
        assert!(body["expansion"]["contains"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["code"] == "kg" && c["display"] == "Kilogramm"));
    }
    supplement["version"] = json!("2");
    supplement["date"] = json!("2026-01-01");
    supplement["concept"][0]["designation"][0]["value"] = json!("Updated milligram");
    publish(&pool, &supplement).await;
    for store in &stores {
        let app = api::create_router(store.clone());
        let (status, body) = request(
            &app,
            "GET",
            "/r4/ValueSet/$expand?url=urn:explicit-ucum:child&displayLanguage=de",
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            body["expansion"]["contains"][0]["display"],
            "Updated milligram"
        );
        let mut historical = child("historical", &["mg"]);
        historical["date"] = json!("2020-01-01");
        for (date, expected) in [
            ("2020", None),
            ("2024-07", Some("milligram")),
            ("2025", Some("Milligramm")),
            ("2026", Some("Updated milligram")),
        ] {
            let (status, body) = request(
                &app,
                "POST",
                "/r4/ValueSet/$expand",
                parameters(vec![
                    json!({"name":"valueSet","resource":historical}),
                    json!({"name":"date","valueDateTime":date}),
                    json!({"name":"displayLanguage","valueCode":"de"}),
                ]),
            )
            .await;
            if let Some(expected) = expected {
                assert_eq!(status, 200, "{body}");
                assert_eq!(
                    body["expansion"]["contains"][0]["display"], expected,
                    "{date}: {body}"
                );
            } else {
                assert_eq!(status, 400, "{body}");
            }
        }
    }
    drop(stores);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ucum_finite_limits_and_unsupported_enumeration_never_publish_or_return_partial_success() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let apps = [
        api::create_router(Arc::new(PostgresStore::new(pool.clone()))),
        api::create_router(Arc::new(PostgresStore::without_prepared_cache(
            pool.clone(),
        ))),
    ];
    for app in &apps {
        for selection in [
            json!({"system":UNITS}),
            json!({"system":UNITS,"filter":[{"property":"code","op":"=","value":"mg"}]}),
            json!({"system":UNITS,"concept":[{"code":"not-a-unit"}]}),
            json!({"system":UNITS,"version":"unavailable","concept":[{"code":"mg"}]}),
            json!({"system":UNITS,"concept":[{"code":"m".repeat(1025)}]}),
        ] {
            let source = json!({"resourceType":"ValueSet","status":"active","compose":{"include":[selection]}});
            let (status, body) = request(
                app,
                "POST",
                "/r4/ValueSet/$expand",
                parameters(vec![json!({"name":"valueSet","resource":source})]),
            )
            .await;
            assert!((400..500).contains(&status), "{body}");
            assert_eq!(body["resourceType"], "OperationOutcome");
            assert!(body.get("expansion").is_none());
        }
        let source = child(
            "too-many",
            &(0..1025)
                .map(|i| format!("1{{fixture{i}}}"))
                .collect::<Vec<_>>()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
        );
        let (status, body) = request(
            app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![json!({"name":"valueSet","resource":source})]),
        )
        .await;
        assert_eq!(status, 400, "{body}");
        assert!(body["issue"][0]["diagnostics"]
            .as_str()
            .unwrap()
            .contains("1024"));
    }
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (0, 0));
    drop(apps);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn bundled_ucum_capabilities_and_subsumption_cannot_inherit_a_stored_instance_hierarchy() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id=publish(&pool,&json!({"resourceType":"CodeSystem","url":UNITS,"version":"2.2","status":"active","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"mg","display":"Stored instance label","concept":[{"code":"g"}]}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for path in [
        "/r4/CodeSystem/$subsumes?system=http://unitsofmeasure.org&codeA=mg&codeB=g",
        "/r4/CodeSystem/$subsumes?system=http://unitsofmeasure.org&version=2.2&codeA=mg&codeB=g",
    ] {
        let (status, body) = request(&app, "GET", path, Value::Null).await;
        assert_eq!(status, 422, "{body}");
    }
    let (status, body) = request(&app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    let units = body["codeSystem"]
        .as_array()
        .unwrap()
        .iter()
        .find(|system| system["uri"] == UNITS)
        .unwrap();
    assert_eq!(units["subsumption"], false);
    let version = units["version"]
        .as_array()
        .unwrap()
        .iter()
        .find(|version| version["code"] == "2.2")
        .unwrap();
    assert_eq!(version["filter"], json!([]));
    assert_eq!(version["compositional"], true);
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "display")["valueString"], "milligram");
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{id}/$lookup?code=mg"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        parameter(&body, "display")["valueString"],
        "Stored instance label",
        "Instance identity remains intact"
    );
    drop(app);
    cleanup(admin, pool, schema).await;
}
