use super::*;

async fn cleanup(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn disabled_retention_has_no_entries_and_preserves_snapshots_and_cross_process_freshness() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut resource = json!({"resourceType":"CodeSystem","url":"urn:codes","version":"1","content":"complete","hierarchyMeaning":"is-a",
        "concept":[{"code":"root","concept":[{"code":"a","display":"Original"}]}]});
    let id = insert_system(&pool, resource.clone()).await;
    insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:codes","version":"1"}]}),
    )
    .await;
    let map = json!({"resourceType":"ConceptMap","url":"urn:map","version":"1","group":[{"source":"urn:codes","target":"urn:codes","element":[{"code":"a","target":[{"code":"a","equivalence":"equal"}]}]}]});
    sqlx::query("INSERT INTO concept_maps(id,url,version,status,content) VALUES($1,'urn:map','1','active',$2)")
        .bind(Uuid::new_v4()).bind(sqlx::types::Json(map)).execute(&pool).await.unwrap();
    let stores = [
        PostgresStore::new(pool.clone()),
        PostgresStore::without_prepared_cache(pool.clone()),
    ];
    let mut held = Vec::new();
    for (index, store) in stores.iter().enumerate() {
        for _ in 0..2 {
            assert_eq!(
                store
                    .resolve_concept("urn:codes", Some("1"), "a")
                    .await
                    .unwrap()
                    .unwrap()
                    .display
                    .as_deref(),
                Some("Original")
            );
            assert_eq!(
                store
                    .resolve_subsumption("urn:codes", Some("1"), "root", "a")
                    .await
                    .unwrap()
                    .unwrap()
                    .relationship,
                Some(true)
            );
            held.push(store.expand_compose("urn:set", Some("1")).await.unwrap());
            store
                .prepare_concept_map("urn:map", Some("1"), None)
                .await
                .unwrap();
        }
        let usage = store.reusable_cache_usage().await;
        if index == 0 {
            assert!(
                usage.iter().all(|(count, bytes)| *count > 0 && *bytes > 0),
                "{usage:?}"
            );
        } else {
            assert_eq!(usage, [(0, 0); 4]);
        }
    }
    resource["concept"][0]["concept"][0]["display"] = "Updated".into();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET content=$2 WHERE id=$1")
        .bind(id)
        .bind(sqlx::types::Json(resource))
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("UPDATE concepts SET display='Updated' WHERE code_system_id=$1 AND code='a'")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for store in &stores {
        assert_eq!(
            store
                .resolve_concept("urn:codes", Some("1"), "a")
                .await
                .unwrap()
                .unwrap()
                .display
                .as_deref(),
            Some("Updated")
        );
        let current = store.expand_compose("urn:set", Some("1")).await.unwrap();
        assert_eq!(
            current
                .members
                .find("urn:codes", Some("1"), "a")
                .unwrap()
                .display
                .as_deref(),
            Some("Updated")
        );
    }
    for previous in held {
        assert_eq!(
            previous
                .members
                .find("urn:codes", Some("1"), "a")
                .unwrap()
                .display
                .as_deref(),
            Some("Original")
        );
    }
    assert_eq!(stores[1].reusable_cache_usage().await, [(0, 0); 4]);
    drop(stores);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn coding_diagnostics_distinguish_case_outside_and_unknown_in_the_selected_edition() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    insert_system(&pool,json!({"resourceType":"CodeSystem","url":"urn:codes","version":"1","caseSensitive":false,"content":"complete","concept":[{"code":"Alpha","display":"Alpha"},{"code":"Outside"}]})).await;
    insert_system(&pool,json!({"resourceType":"CodeSystem","url":"urn:codes","version":"2","content":"complete","concept":[{"code":"Missing"}]})).await;
    insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:codes","version":"1","concept":[{"code":"Alpha"}]}]}),
    )
    .await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    for (code, valid, kinds) in [
        ("alpha", true, vec!["code-rule"]),
        ("Outside", false, vec!["not-in-vs"]),
        ("Missing", false, vec!["not-in-vs", "invalid-code"]),
    ] {
        let body = json!({"resourceType":"Parameters","parameter":[{"name":"url","valueUri":"urn:set"},{"name":"coding","valueCoding":{"system":"urn:codes","code":code}}]});
        let (status, result) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code?nocache=123",
            body,
        )
        .await;
        assert_eq!(status, 200, "{result}");
        assert_eq!(
            parameter(&result, "result")["valueBoolean"],
            valid,
            "{result}"
        );
        assert_eq!(parameter(&result, "system")["valueUri"], "urn:codes");
        assert_eq!(parameter(&result, "code")["valueCode"], code);
        assert_eq!(parameter(&result, "version")["valueString"], "1");
        let issues = parameter(&result, "issues")["resource"]["issue"]
            .as_array()
            .unwrap();
        assert_eq!(
            issues
                .iter()
                .map(|issue| issue["details"]["coding"][0]["code"].as_str().unwrap())
                .collect::<Vec<_>>(),
            kinds
        );
        if valid {
            assert_eq!(parameter(&result, "normalized-code")["valueCode"], "Alpha");
            assert!(
                store.reusable_cache_usage().await[2].0 > 0,
                "Positive Coding validation must retain the prepared fast path"
            );
        }
    }
    let body = json!({"resourceType":"Parameters","parameter":[{"name":"url","valueUri":"urn:set"},{"name":"coding","valueCoding":{"system":"urn:codes","version":"2","code":"Missing"}}]});
    let (status, result) = request(&app, "POST", "/r4/ValueSet/$validate-code", body).await;
    assert_eq!(status, 200, "{result}");
    assert_eq!(parameter(&result, "result")["valueBoolean"], false);
    assert_eq!(parameter(&result, "version")["valueString"], "2");
    let issues = parameter(&result, "issues")["resource"]["issue"]
        .as_array()
        .unwrap();
    assert_eq!(
        issues.len(),
        1,
        "A known code in a different edition is outside this set, not an unknown code"
    );
    assert_eq!(issues[0]["details"]["coding"][0]["code"], "not-in-vs");
    let (status, body) = request(
        &app,
        "GET",
        "/r4/metadata?nocache=123&_format=json",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["fhirVersion"], "4.0.1");
    for path in [
        "/r4/metadata?nocache=1&nocache=2",
        "/r4/metadata?nocache=-1",
        "/r4/metadata?unknown=1",
    ] {
        assert_eq!(request(&app, "GET", path, Value::Null).await.0, 400);
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}
