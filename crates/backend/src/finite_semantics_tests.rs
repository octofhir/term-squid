//! Shared finite R4 semantics must agree across stored, inline and request-local forms.
use super::*;
fn parameters(fields: Vec<Value>) -> Value {
    json!({"resourceType":"Parameters","parameter":fields})
}
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
async fn formal_boolean_meanings_and_status_agree_for_stored_inline_and_temporary_validation() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut source = json!({"resourceType":"CodeSystem","url":"urn:meanings","version":"1","status":"active","content":"complete","property":[{"code":"grouping","uri":"http://hl7.org/fhir/concept-properties#notSelectable","type":"boolean"},{"code":"life","uri":"http://hl7.org/fhir/concept-properties#status","type":"code"}],"concept":[{"code":"Unknown"},{"code":"Selectable","property":[{"code":"grouping","valueBoolean":false}]},{"code":"Group","property":[{"code":"grouping","valueBoolean":true}]},{"code":"Retired","property":[{"code":"life","valueCode":"retired"}]}]});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut transaction = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut transaction, &source, &inspection)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    insert_set(&pool,Some("1"),json!({"include":[{"system":"urn:meanings","version":"1","filter":[{"property":"grouping","op":"=","value":"false"}]}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=urn:set&activeOnly=true",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 2);
    assert_eq!(
        body["expansion"]["contains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|concept| concept["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Selectable", "Unknown"]
    );
    for mode in ["stored", "inline", "temporary"] {
        for (code, valid) in [("Unknown", true), ("Selectable", true), ("Group", false)] {
            let mut fields = vec![
                json!({"name":"code","valueCode":code}),
                json!({"name":"abstract","valueBoolean":false}),
            ];
            if mode == "inline" {
                fields.push(json!({"name":"codeSystem","resource":source}));
            } else {
                fields.push(json!({"name":"url","valueUri":"urn:meanings"}));
            }
            if mode == "temporary" {
                fields.push(json!({"name":"tx-resource","resource":source}));
            }
            let (status, body) = request(
                &app,
                "POST",
                "/r4/CodeSystem/$validate-code",
                parameters(fields),
            )
            .await;
            assert_eq!(status, 200, "{mode}: {body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                valid,
                "{mode}: {body}"
            );
        }
    }
    source["url"] = json!("urn:custom-flag");
    source["property"][0]["uri"] = json!("urn:unrelated-boolean");
    source["property"][0]["code"] = json!("notSelectable");
    source["concept"][2]["property"][0]["code"] = json!("notSelectable");
    for mode in ["inline", "temporary"] {
        let mut fields = vec![
            json!({"name":"code","valueCode":"Group"}),
            json!({"name":"abstract","valueBoolean":false}),
        ];
        if mode == "inline" {
            fields.push(json!({"name":"codeSystem","resource":source}));
        } else {
            fields.push(json!({"name":"url","valueUri":"urn:custom-flag"}));
            fields.push(json!({"name":"tx-resource","resource":source}));
        }
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$validate-code",
            parameters(fields),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            true,
            "Unrelated property meaning must not reject this code: {body}"
        );
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn bounded_scalar_filters_work_through_public_stored_and_temporary_operations() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let source = json!({"resourceType":"CodeSystem","url":"urn:filter-http","version":"1","status":"active","caseSensitive":false,"content":"complete","hierarchyMeaning":"is-a","property":[{"code":"tag","type":"code"},{"code":"rank","type":"integer"}],"concept":[{"code":"Root","concept":[{"code":"Alpha","display":"Alpha display","property":[{"code":"tag","valueCode":"two"},{"code":"rank","valueInteger":2}]},{"code":"Beta","display":"Beta display","property":[{"code":"tag","valueCode":"other"},{"code":"rank","valueInteger":3}]}]}]});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut transaction = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut transaction, &source, &inspection)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for temporary in [false, true] {
        for (property, op, value, total) in [
            ("code", "regex", "alpha|beta", 2),
            ("rank", "in", "1,2", 1),
            ("tag", "not-in", "two", 2),
            ("concept", "child-of", "Root", 2),
        ] {
            let set = json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:filter-http","version":"1","filter":[{"property":property,"op":op,"value":value}]}]}});
            let mut fields = vec![json!({"name":"valueSet","resource":set})];
            if temporary {
                fields.push(json!({"name":"tx-resource","resource":source}));
            }
            let (status, body) =
                request(&app, "POST", "/r4/ValueSet/$expand", parameters(fields)).await;
            assert_eq!(status, 200, "{property} {op}: {body}");
            assert_eq!(body["expansion"]["total"], total, "{body}");
        }
        let set = json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:filter-http","version":"1","filter":[{"property":"code","op":"regex","value":"alpha|beta"}]}]}});
        let mut fields = vec![
            json!({"name":"valueSet","resource":set}),
            json!({"name":"coding","valueCoding":{"system":"urn:filter-http","code":"alpha"}}),
        ];
        if temporary {
            fields.push(json!({"name":"tx-resource","resource":source}));
        }
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(fields),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    }
    let (status,body)=request(&app,"POST","/r4/ValueSet/$expand",parameters(vec![json!({"name":"valueSet","resource":{"resourceType":"ValueSet","compose":{"include":[{"system":"urn:filter-http","filter":[{"property":"code","op":"regex","value":"(?<=x)y"}]}]}}})])).await;
    assert_eq!(status, 422, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    let (status, body) = request(&app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    let system = body["codeSystem"]
        .as_array()
        .unwrap()
        .iter()
        .find(|system| system["uri"] == "urn:filter-http")
        .unwrap();
    let filters = system["version"][0]["filter"].as_array().unwrap();
    assert!(filters.iter().any(|filter| filter["code"] == "code"
        && filter["op"].as_array().unwrap().contains(&json!("regex"))));
    assert!(!filters.iter().any(|filter| filter["code"] == "rank"
        && filter["op"].as_array().unwrap().contains(&json!("regex"))));
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn validation_version_directives_reuse_expansion_semantics_without_overriding_coding_assertions(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    for version in ["1", "2"] {
        let source = json!({"resourceType":"CodeSystem","url":"urn:validation-version","version":version,"status":"active","content":"complete","concept":[{"code":"same","display":format!("Edition {version}")} ]});
        let inspection = term_squid_ingestion::inspect(&source).unwrap();
        let mut tx = pool.begin().await.unwrap();
        term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:validation-version","version":"1"}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (name, version, expected) in [
        ("system-version", "2", "1"),
        ("force-system-version", "2", "2"),
        ("check-system-version", "1", "1"),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(vec![
                json!({"name":"url","valueUri":"urn:set"}),
                json!({"name":"system","valueUri":"urn:validation-version"}),
                json!({"name":"code","valueCode":"same"}),
                json!({"name":name,"valueCanonical":format!("urn:validation-version|{version}")}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        assert_eq!(parameter(&body, "version")["valueString"], expected);
        let (status,body)=request(&app,"GET",&format!("/r4/ValueSet/$validate-code?url=urn:set&system=urn:validation-version&code=same&{name}=urn:validation-version%7C{version}"),Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "version")["valueString"], expected);
    }
    let (status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",parameters(vec![json!({"name":"url","valueUri":"urn:set"}),json!({"name":"coding","valueCoding":{"system":"urn:validation-version","version":"1","code":"same"}}),json!({"name":"force-system-version","valueCanonical":"urn:validation-version|2"})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        parameter(&body, "result")["valueBoolean"],
        false,
        "A directive cannot turn a version-1 Coding into a version-2 Coding: {body}"
    );
    for parameters in [
        vec![json!({"name":"check-system-version","valueCanonical":"urn:validation-version|2"})],
        vec![
            json!({"name":"system-version","valueCanonical":"urn:validation-version|1"}),
            json!({"name":"system-version","valueCanonical":"urn:validation-version|2"}),
        ],
    ] {
        let mut fields = vec![
            json!({"name":"url","valueUri":"urn:set"}),
            json!({"name":"system","valueUri":"urn:validation-version"}),
            json!({"name":"code","valueCode":"same"}),
        ];
        fields.extend(parameters);
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            self::parameters(fields),
        )
        .await;
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn cached_negative_diagnostics_keep_one_selected_snapshot_and_refresh_code_knowledge_after_commit(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut source = json!({"resourceType":"CodeSystem","url":"urn:knowledge","version":"1","status":"active","content":"complete","concept":[{"code":"Included"},{"code":"Outside"}]});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:knowledge","concept":[{"code":"Included"}]}]}),
    )
    .await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    let held = store.expand_compose("urn:set", None).await.unwrap();
    let key = term_squid_core::Canonical::parse("urn:knowledge", Some("1")).unwrap();
    assert!(held.knowledge[&key].complete());
    assert_eq!(held.knowledge[&key].resolve("Missing"), None);
    for (code, count) in [("Outside", 1), ("Missing", 2), ("Missing", 2)] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(vec![
                json!({"name":"url","valueUri":"urn:set"}),
                json!({"name":"coding","valueCoding":{"system":"urn:knowledge","code":code}}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], false);
        assert_eq!(
            parameter(&body, "issues")["resource"]["issue"]
                .as_array()
                .unwrap()
                .len(),
            count,
            "{body}"
        );
    }
    source["version"] = json!("2");
    source["concept"]
        .as_array_mut()
        .unwrap()
        .push(json!({"code":"Missing"}));
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$validate-code",
        parameters(vec![
            json!({"name":"url","valueUri":"urn:set"}),
            json!({"name":"coding","valueCoding":{"system":"urn:knowledge","code":"Missing"}}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    assert_eq!(
        parameter(&body, "issues")["resource"]["issue"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "A committed known code outside the set must stop being reported as unknown: {body}"
    );
    assert_eq!(
        held.knowledge[&key].resolve("Missing"),
        None,
        "Previously issued snapshots remain immutable"
    );
    assert_eq!(
        store
            .expand_compose("urn:set", None)
            .await
            .unwrap()
            .knowledge[&term_squid_core::Canonical::parse("urn:knowledge", Some("2")).unwrap()]
            .resolve("Missing"),
        Some("Missing")
    );
    assert_eq!(
        store.concept_cache_usage().await,
        (0, 0),
        "Diagnostics resolve from the selected ValueSet snapshot, not another latest-system cache"
    );
    drop(app);
    drop(store);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn codeable_concept_diagnostics_report_the_selected_coding_and_its_actual_field_path() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let source = json!({"resourceType":"CodeSystem","url":"urn:object-code","version":"1","status":"active","content":"complete","caseSensitive":false,"concept":[{"code":"Alpha","display":"Authoritative display"}]});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:object-code","version":"1"}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (display, valid) in [("Authoritative display", true), ("Wrong display", false)] {
        let (status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",parameters(vec![json!({"name":"url","valueUri":"urn:set"}),json!({"name":"codeableConcept","valueCodeableConcept":{"coding":[{"system":"urn:outside","code":"unrelated"},{"system":"urn:object-code","version":"1","code":"alpha","display":display}]}})])).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
        assert_eq!(parameter(&body, "code")["valueCode"], "alpha");
        assert_eq!(parameter(&body, "system")["valueUri"], "urn:object-code");
        assert_eq!(parameter(&body, "normalized-code")["valueCode"], "Alpha");
        let issues = parameter(&body, "issues")["resource"]["issue"]
            .as_array()
            .unwrap();
        assert!(
            issues
                .iter()
                .any(|issue| issue["details"]["coding"][0]["code"] == "code-rule"
                    && issue["expression"] == json!(["CodeableConcept.coding[1].code"])),
            "{body}"
        );
        if !valid {
            assert!(
                issues
                    .iter()
                    .any(
                        |issue| issue["details"]["coding"][0]["code"] == "invalid-display"
                            && issue["expression"] == json!(["CodeableConcept.coding[1].display"])
                    ),
                "{body}"
            );
        }
    }
    let (status,body)=request(&app,"POST","/r4/ValueSet/$validate-code",parameters(vec![json!({"name":"url","valueUri":"urn:set"}),json!({"name":"codeableConcept","valueCodeableConcept":{"coding":[{"system":"urn:object-code","code":"Missing"}]}})])).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    assert_eq!(
        parameter(&body, "issues")["resource"]["issue"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(parameter(&body, "issues")["resource"]["issue"]
        .as_array()
        .unwrap()
        .iter()
        .all(|issue| issue["expression"] == json!(["CodeableConcept.coding[0].code"])));
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn display_validation_accepts_real_designations_across_api_forms_and_respects_language() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let source = json!({"resourceType":"CodeSystem","url":"urn:labels","version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"Primary label","designation":[{"language":"en","value":"English synonym"},{"language":"fr","value":"French synonym"}]}]});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let id = term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    insert_set(&pool, Some("1"), json!({"include":[{"system":"urn:labels","version":"1","concept":[{"code":"A","display":"ValueSet preferred label"}]}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for target in ["CodeSystem", "ValueSet"] {
        for (display, valid) in [
            ("Primary label", true),
            ("English synonym", true),
            ("French synonym", true),
            ("Unregistered label", false),
        ] {
            let url = if target == "CodeSystem" {
                "urn:labels"
            } else {
                "urn:set"
            };
            let encoded = display.replace(' ', "%20");
            let (status, body) = request(&app, "GET", &format!("/r4/{target}/$validate-code?url={url}&system=urn:labels&code=A&display={encoded}"), Value::Null).await;
            assert_eq!(status, 200, "{target} {display}: {body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
            for coding in [false, true] {
                let mut fields = vec![json!({"name":"url","valueUri":url})];
                if coding {
                    fields.push(json!({"name":"coding","valueCoding":{"system":"urn:labels","code":"A","display":display}}));
                } else {
                    fields.extend([
                        json!({"name":"system","valueUri":"urn:labels"}),
                        json!({"name":"code","valueCode":"A"}),
                        json!({"name":"display","valueString":display}),
                    ]);
                }
                let (status, body) = request(
                    &app,
                    "POST",
                    &format!("/r4/{target}/$validate-code"),
                    parameters(fields),
                )
                .await;
                assert_eq!(status, 200, "{body}");
                assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
            }
        }
    }
    let id = id.id;
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{id}/$validate-code?code=A&display=English%20synonym"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    for mode in ["stored", "inline", "temporary"] {
        for (display, language, valid) in [
            ("French synonym", None, true),
            ("French synonym", Some("en"), false),
            ("English synonym", Some("en"), true),
        ] {
            let mut fields = vec![
                json!({"name":"code","valueCode":"A"}),
                json!({"name":"display","valueString":display}),
            ];
            if mode == "inline" {
                fields.push(json!({"name":"codeSystem","resource":source}));
            } else {
                fields.push(json!({"name":"url","valueUri":"urn:labels"}));
            }
            if mode == "temporary" {
                fields.push(json!({"name":"tx-resource","resource":source}));
            }
            if let Some(language) = language {
                fields.push(json!({"name":"displayLanguage","valueCode":language}));
            }
            let (status, body) = request(
                &app,
                "POST",
                "/r4/CodeSystem/$validate-code",
                parameters(fields),
            )
            .await;
            assert_eq!(status, 200, "{mode}: {body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                valid,
                "{mode}: {body}"
            );
        }
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn contained_valuesets_are_scoped_to_their_container_and_preserve_original_json() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let source = json!({"resourceType":"CodeSystem","url":"urn:contained-codes","status":"active","content":"complete","concept":[{"code":"A"},{"code":"B"},{"code":"C"}]});
    let child = |code| json!({"resourceType":"ValueSet","id":"same-id","url":"urn:same-child-url","status":"active","compose":{"include":[{"system":"urn:contained-codes","concept":[{"code":code}]}]}});
    let container = |url, code| json!({"resourceType":"ValueSet","url":url,"version":"1","status":"active","contained":[child(code)],"compose":{"include":[{"valueSet":["#same-id"]}]}});
    let left = container("urn:left-container", "A");
    let right = container("urn:right-container", "B");
    let root = json!({"resourceType":"ValueSet","url":"urn:contained-root","status":"active","compose":{"include":[{"valueSet":["urn:left-container|1"]},{"valueSet":["urn:right-container|1"]}]}});
    let mut tx = pool.begin().await.unwrap();
    for resource in [&source, &left, &right, &root] {
        let inspection = term_squid_ingestion::inspect(resource).unwrap();
        term_squid_ingestion::import_resource(&mut tx, resource, &inspection)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for _ in 0..2 {
        let (status, body) = request(
            &app,
            "GET",
            "/r4/ValueSet/$expand?url=urn:contained-root",
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 2);
        assert_eq!(
            body["expansion"]["contains"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["code"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert!(
            !body.to_string().contains("urn:term-squid:contained:"),
            "Internal evaluation identities are not public provenance: {body}"
        );
    }
    for mode in ["inline", "temporary"] {
        let mut fields = if mode == "inline" {
            vec![json!({"name":"valueSet","resource":left})]
        } else {
            vec![
                json!({"name":"url","valueUri":"urn:left-container"}),
                json!({"name":"tx-resource","resource":left}),
            ]
        };
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(fields.clone()),
        )
        .await;
        assert_eq!(status, 200, "{mode}: {body}");
        assert_eq!(body["expansion"]["total"], 1);
        assert_eq!(body["expansion"]["contains"][0]["code"], "A");
        assert_eq!(body["compose"], left["compose"]);
        assert_eq!(body["contained"], left["contained"]);
        let mut inferred = fields.clone();
        inferred.push(json!({"name":"code","valueCode":"A"}));
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(inferred),
        )
        .await;
        assert_eq!(status, 200, "Inferred contained context: {body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        fields.extend([
            json!({"name":"system","valueUri":"urn:contained-codes"}),
            json!({"name":"code","valueCode":"B"}),
        ]);
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(fields),
        )
        .await;
        assert_eq!(status, 200, "{mode}: {body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    }
    let mut broken = left.clone();
    broken["compose"]["include"][0]["valueSet"] = json!(["#missing"]);
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        parameters(vec![json!({"name":"valueSet","resource":broken})]),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(body
        .to_string()
        .contains("Unresolved local ValueSet reference"));
    let mut cycle = left;
    cycle["contained"][0]["compose"] = json!({"include":[{"valueSet":["#"]}]});
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        parameters(vec![json!({"name":"valueSet","resource":cycle})]),
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert!(body.to_string().to_lowercase().contains("cycle"));
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn lenient_display_policy_warns_without_promoting_membership_or_abstract_failures() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let source = json!({"resourceType":"CodeSystem","url":"urn:display-policy","status":"active","content":"complete","concept":[{"code":"A","display":"Actual label"},{"code":"Group","display":"Grouping","property":[{"code":"notSelectable","valueBoolean":true}]}]});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:display-policy"}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for target in ["CodeSystem", "ValueSet"] {
        let url = if target == "CodeSystem" {
            "urn:display-policy"
        } else {
            "urn:set"
        };
        for (code, lenient, allow_abstract, valid) in [
            ("A", false, true, false),
            ("A", true, true, true),
            ("Missing", true, true, false),
            ("Group", true, false, false),
        ] {
            let fields = vec![
                json!({"name":"url","valueUri":url}),
                json!({"name":"coding","valueCoding":{"system":"urn:display-policy","code":code,"display":"Incorrect label"}}),
                json!({"name":"lenient-display-validation","valueBoolean":lenient}),
                json!({"name":"abstract","valueBoolean":allow_abstract}),
            ];
            let (status, body) = request(
                &app,
                "POST",
                &format!("/r4/{target}/$validate-code"),
                parameters(fields),
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
            let issues = parameter(&body, "issues")["resource"]["issue"]
                .as_array()
                .unwrap();
            if valid {
                assert_eq!(parameter(&body, "display")["valueString"], "Actual label");
                assert!(
                    issues.iter().all(|issue| issue["severity"] == "warning"),
                    "{body}"
                );
                assert_eq!(issues[0]["details"]["coding"][0]["code"], "invalid-display");
            } else {
                assert!(
                    issues.iter().any(|issue| issue["severity"] == "error"),
                    "{body}"
                );
            }
        }
        let (status,body)=request(&app,"GET",&format!("/r4/{target}/$validate-code?url={url}&system=urn:display-policy&code=A&display=Incorrect&lenient-display-validation=true"),Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        assert_eq!(
            parameter(&body, "issues")["resource"]["issue"][0]["expression"],
            json!(["display"])
        );
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
async fn lenient_display_policy_preserves_sql_free_ucum_and_inline_validation() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    for (code, valid) in [("mg", true), ("not-a-unit", false)] {
        let (status,body)=request(&app,"POST","/r4/CodeSystem/$validate-code",parameters(vec![json!({"name":"url","valueUri":"http://unitsofmeasure.org"}),json!({"name":"coding","valueCoding":{"system":"http://unitsofmeasure.org","code":code,"display":"Incorrect label"}}),json!({"name":"lenient-display-validation","valueBoolean":true})])).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
    }
    for (mode, code, status, valid) in [
        ("complete", "A", 200, true),
        ("complete", "Missing", 200, false),
        ("fragment", "Missing", 400, false),
    ] {
        let resource = json!({"resourceType":"CodeSystem","url":"urn:local-display","status":"active","content":mode,"concept":[{"code":"A","display":"Actual"}]});
        let (actual, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$validate-code",
            parameters(vec![
                json!({"name":"codeSystem","resource":resource}),
                json!({"name":"code","valueCode":code}),
                json!({"name":"display","valueString":"Incorrect"}),
                json!({"name":"lenient-display-validation","valueBoolean":true}),
            ]),
        )
        .await;
        assert_eq!(actual, status, "{body}");
        if actual == 200 {
            assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
        }
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn abstract_policy_does_not_treat_mistyped_legacy_or_temporary_flags_as_false() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let source = json!({"resourceType":"CodeSystem","url":"urn:unsafe-flag","status":"active","content":"complete","property":[{"code":"grouping","uri":"http://hl7.org/fhir/concept-properties#notSelectable","type":"boolean"}],"concept":[{"code":"A","property":[{"code":"grouping","valueCode":"true"}]}]});
    let inspection = term_squid_ingestion::inspect_cli(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for mode in ["legacy", "inline", "temporary"] {
        let mut fields = vec![
            json!({"name":"code","valueCode":"A"}),
            json!({"name":"abstract","valueBoolean":false}),
            json!({"name":"lenient-display-validation","valueBoolean":true}),
        ];
        if mode == "inline" {
            fields.push(json!({"name":"codeSystem","resource":source}));
        } else {
            fields.push(json!({"name":"url","valueUri":"urn:unsafe-flag"}));
        }
        if mode == "temporary" {
            fields.push(json!({"name":"tx-resource","resource":source}));
        }
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$validate-code",
            parameters(fields),
        )
        .await;
        assert_eq!(status, 422, "{mode}: {body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
        assert!(
            body["issue"][0]["diagnostics"]
                .as_str()
                .unwrap()
                .contains("boolean")
                || body["issue"][0]["diagnostics"]
                    .as_str()
                    .unwrap()
                    .contains("Boolean"),
            "{body}"
        );
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn incomplete_finite_hierarchies_do_not_advertise_or_return_definitive_subsumption() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut source = json!({"resourceType":"CodeSystem","url":"urn:partial-hierarchy","version":"1","status":"active","content":"fragment","hierarchyMeaning":"is-a","concept":[{"code":"A","concept":[{"code":"B"}]},{"code":"C"}]});
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    let id = term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap()
        .id;
    tx.commit().await.unwrap();
    let apps = [
        api::create_router(Arc::new(PostgresStore::new(pool.clone()))),
        api::create_router(Arc::new(PostgresStore::without_prepared_cache(
            pool.clone(),
        ))),
    ];
    for app in &apps {
        for (a, b) in [("A", "B"), ("A", "C"), ("Missing", "A")] {
            let (status, body) = request(
                app,
                "GET",
                &format!(
                    "/r4/CodeSystem/$subsumes?system=urn:partial-hierarchy&codeA={a}&codeB={b}"
                ),
                Value::Null,
            )
            .await;
            assert_eq!(status, 422, "{body}");
            assert_eq!(body["resourceType"], "OperationOutcome");
            assert!(body.to_string().contains("indeterminate"));
        }
        let (status, body) = request(
            app,
            "GET",
            &format!("/r4/CodeSystem/{id}/$subsumes?codeA=A&codeB=C"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 422, "{body}");
        let (status,body)=request(app,"POST","/r4/CodeSystem/$subsumes",parameters(vec![json!({"name":"codingA","valueCoding":{"system":"urn:partial-hierarchy","version":"1","code":"A"}}),json!({"name":"codingB","valueCoding":{"system":"urn:partial-hierarchy","version":"1","code":"C"}})])).await;
        assert_eq!(status, 422, "{body}");
        let (status, body) = request(
            app,
            "GET",
            "/r4/CodeSystem/$validate-code?url=urn:partial-hierarchy&version=1&code=A",
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "Known projected codes remain usable: {body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        let (status, body) =
            request(app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
        assert_eq!(status, 200, "{body}");
        let inventory = body["codeSystem"]
            .as_array()
            .unwrap()
            .iter()
            .find(|system| system["uri"] == "urn:partial-hierarchy")
            .unwrap();
        assert_eq!(inventory["subsumption"], false);
        assert_eq!(inventory["version"][0]["filter"], json!([]));
    }
    source["version"] = json!("2");
    source["content"] = json!("complete");
    let inspection = term_squid_ingestion::inspect(&source).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for app in &apps {
        for (a, b, outcome) in [("A", "C", "not-subsumed"), ("A", "B", "subsumes")] {
            let (status, body) = request(
                app,
                "GET",
                &format!(
                    "/r4/CodeSystem/$subsumes?system=urn:partial-hierarchy&codeA={a}&codeB={b}"
                ),
                Value::Null,
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "outcome")["valueCode"], outcome);
        }
        let (status, body) = request(
            app,
            "GET",
            "/r4/CodeSystem/$subsumes?system=urn:partial-hierarchy&version=1&codeA=A&codeB=C",
            Value::Null,
        )
        .await;
        assert_eq!(
            status, 422,
            "The incomplete immutable edition remains indeterminate: {body}"
        );
        let (_, body) = request(app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
        let inventory = body["codeSystem"]
            .as_array()
            .unwrap()
            .iter()
            .find(|system| system["uri"] == "urn:partial-hierarchy")
            .unwrap();
        assert_eq!(inventory["subsumption"], true);
        let versions = inventory["version"].as_array().unwrap();
        assert_eq!(
            versions
                .iter()
                .find(|version| version["code"] == "1")
                .unwrap()["filter"],
            json!([])
        );
        assert!(!versions
            .iter()
            .find(|version| version["code"] == "2")
            .unwrap()["filter"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    drop(apps);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn display_fallback_resolves_original_selectors_on_one_complete_post_activation_snapshot() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    for retention in [true, false] {
        for flow in [
            "cs",
            "cs-coding",
            "vs",
            "instance",
            "cs-pinned",
            "vs-pinned",
            "vs-edition",
        ] {
            let system = format!("urn:label-snapshot:{retention}:{flow}");
            let set = format!("{system}:set");
            let mut source = json!({"resourceType":"CodeSystem","url":system,"version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"First display","designation":[{"language":"en","value":"Accepted alias"}]}]});
            let inspection = term_squid_ingestion::inspect(&source).unwrap();
            let mut tx = pool.begin().await.unwrap();
            let id = term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
                .await
                .unwrap()
                .id;
            let definition = json!({"resourceType":"ValueSet","url":set,"version":"1","status":"active","compose":{"include":[{"system":system}]}});
            let inspection = term_squid_ingestion::inspect(&definition).unwrap();
            term_squid_ingestion::import_resource(&mut tx, &definition, &inspection)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            let store = Arc::new(if retention {
                PostgresStore::new(pool.clone())
            } else {
                PostgresStore::without_prepared_cache(pool.clone())
            });
            let app = api::create_router(store.clone());
            let fixed = matches!(flow, "instance" | "cs-pinned" | "vs-pinned");
            for (phase, display) in [(2, "Accepted alias"), (3, "New alias")] {
                let mut query = form_urlencoded::Serializer::new(String::new());
                query
                    .append_pair("code", "A")
                    .append_pair("display", display);
                let path = if flow == "instance" {
                    format!("/r4/CodeSystem/{id}/$validate-code?{}", query.finish())
                } else if flow.starts_with("vs") {
                    query
                        .append_pair("url", &set)
                        .append_pair("system", &system);
                    if flow == "vs-pinned" {
                        query.append_pair("systemVersion", "1");
                    }
                    if flow == "vs-edition" {
                        query.append_pair("valueSetVersion", "1");
                    }
                    format!("/r4/ValueSet/$validate-code?{}", query.finish())
                } else {
                    query.append_pair("url", &system);
                    if flow == "cs-pinned" {
                        query.append_pair("version", "1");
                    }
                    format!("/r4/CodeSystem/$validate-code?{}", query.finish())
                };
                let (reached, resume) = store.gate_next_probe().await;
                let request_app = app.clone();
                let fields = parameters(vec![
                    json!({"name":"url","valueUri":system}),
                    json!({"name":"coding","valueCoding":{"system":system,"code":"A","display":display}}),
                ]);
                let pending = tokio::spawn(async move {
                    if flow == "cs-coding" {
                        request(
                            &request_app,
                            "POST",
                            "/r4/CodeSystem/$validate-code",
                            fields,
                        )
                        .await
                    } else {
                        request(&request_app, "GET", &path, Value::Null).await
                    }
                });
                tokio::time::timeout(std::time::Duration::from_secs(10), reached.notified())
                    .await
                    .expect("The first probe must finish before publication");
                source["version"] = json!(phase.to_string());
                source["concept"][0]["display"] = json!(if phase == 2 {
                    "Second display"
                } else {
                    "Third display"
                });
                source["concept"][0]["designation"] = if phase == 2 {
                    json!([])
                } else {
                    json!([{"language":"en","value":"New alias"}])
                };
                let inspection = term_squid_ingestion::inspect(&source).unwrap();
                let mut tx = pool.begin().await.unwrap();
                term_squid_ingestion::import_resource(&mut tx, &source, &inspection)
                    .await
                    .unwrap();
                tx.commit().await.unwrap();
                resume.notify_one();
                let (status, body) =
                    tokio::time::timeout(std::time::Duration::from_secs(10), pending)
                        .await
                        .unwrap()
                        .unwrap();
                assert_eq!(status, 200, "{retention} {flow} phase {phase}: {body}");
                assert_eq!(
                    parameter(&body, "result")["valueBoolean"],
                    if fixed { phase == 2 } else { phase == 3 },
                    "{retention} {flow} phase {phase}: {body}"
                );
                assert_eq!(
                    parameter(&body, "version")["valueString"],
                    if fixed {
                        "1".to_owned()
                    } else {
                        phase.to_string()
                    },
                    "{retention} {flow}: {body}"
                );
                assert_eq!(
                    parameter(&body, "display")["valueString"],
                    if fixed {
                        "First display"
                    } else if phase == 2 {
                        "Second display"
                    } else {
                        "Third display"
                    },
                    "{retention} {flow}: {body}"
                );
            }
            drop(app);
            drop(store);
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn required_supplements_are_real_snapshot_dependencies_for_all_finite_set_forms() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    async fn publish(pool: &PgPool, source: &Value) -> Uuid {
        let inspection = term_squid_ingestion::inspect(source).unwrap();
        let mut tx = pool.begin().await.unwrap();
        let id = term_squid_ingestion::import_resource(&mut tx, source, &inspection)
            .await
            .unwrap()
            .id;
        tx.commit().await.unwrap();
        id
    }
    publish(&pool, &json!({"resourceType":"CodeSystem","url":"urn:required:codes","version":"1","status":"active","date":"2026-01-01","content":"complete","concept":[{"code":"A","display":"Alpha"}]})).await;
    let definition = json!({"resourceType":"ValueSet","url":"urn:required:set","version":"1","status":"active","extension":[{"url":"http://hl7.org/fhir/StructureDefinition/valueset-supplement","valueCanonical":"urn:required:supplement|1"}],"compose":{"include":[{"system":"urn:required:codes","version":"1"}]}});
    let id = publish(&pool, &definition).await;
    let wrapper = json!({"resourceType":"ValueSet","url":"urn:required:wrapper","version":"1","status":"active","compose":{"include":[{"valueSet":["urn:required:set|1"]}]}});
    publish(&pool, &wrapper).await;
    let mut child = definition.clone();
    child["id"] = json!("child");
    let contained = json!({"resourceType":"ValueSet","contained":[child],"compose":{"include":[{"valueSet":["#child"]}]}});
    let stores = [
        Arc::new(PostgresStore::new(pool.clone())),
        Arc::new(PostgresStore::without_prepared_cache(pool.clone())),
    ];
    let selectors = vec![
        (
            "stored",
            vec![json!({"name":"url","valueUri":"urn:required:set"})],
        ),
        (
            "inline",
            vec![json!({"name":"valueSet","resource":definition})],
        ),
        (
            "temporary",
            vec![
                json!({"name":"url","valueUri":"urn:required:set"}),
                json!({"name":"tx-resource","resource":definition}),
            ],
        ),
        (
            "imported",
            vec![json!({"name":"url","valueUri":"urn:required:wrapper"})],
        ),
        (
            "contained",
            vec![json!({"name":"valueSet","resource":contained})],
        ),
    ];
    for available in [false, true] {
        if available {
            publish(&pool, &json!({"resourceType":"CodeSystem","url":"urn:required:supplement","version":"1","status":"active","date":"2026-01-01","content":"supplement","supplements":"urn:required:codes|1","concept":[{"code":"A","designation":[{"language":"ru","value":"Required alias"}]}]})).await;
        }
        for store in &stores {
            let app = api::create_router(store.clone());
            for (name, selector) in &selectors {
                let (status, body) = request(
                    &app,
                    "POST",
                    "/r4/ValueSet/$expand",
                    parameters(selector.clone()),
                )
                .await;
                assert_eq!(status, if available { 200 } else { 422 }, "{name}: {body}");
                if available {
                    assert_eq!(body["expansion"]["total"], 1);
                    assert!(body["expansion"]["parameter"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p["name"] == "used-supplement"
                            && p["valueUri"] == "urn:required:supplement|1"));
                } else {
                    assert!(body
                        .to_string()
                        .contains("Required supplement urn:required:supplement|1"));
                }
                for input in [
                    vec![
                        json!({"name":"system","valueUri":"urn:required:codes"}),
                        json!({"name":"code","valueCode":"A"}),
                    ],
                    vec![
                        json!({"name":"coding","valueCoding":{"system":"urn:required:codes","code":"A"}}),
                    ],
                    vec![
                        json!({"name":"codeableConcept","valueCodeableConcept":{"coding":[{"system":"urn:required:codes","code":"A"}]}}),
                    ],
                ] {
                    let mut fields = selector.clone();
                    fields.extend(input);
                    let (status, body) = request(
                        &app,
                        "POST",
                        "/r4/ValueSet/$validate-code",
                        parameters(fields),
                    )
                    .await;
                    assert_eq!(status, if available { 200 } else { 422 }, "{name}: {body}");
                    if available {
                        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
                    }
                }
            }
            let (status, body) = request(
                &app,
                "GET",
                &format!("/r4/ValueSet/{id}/$expand?includeDesignations=true"),
                Value::Null,
            )
            .await;
            assert_eq!(status, if available { 200 } else { 422 }, "{body}");
            if available {
                assert_eq!(
                    body["expansion"]["contains"][0]["designation"][0]["value"],
                    "Required alias"
                );
            }
        }
    }
    // A newer automatic edition cannot silently fulfill an explicit requirement for 1.
    publish(&pool, &json!({"resourceType":"CodeSystem","url":"urn:required:supplement","version":"2","status":"active","date":"2026-02-01","content":"supplement","supplements":"urn:required:codes|1","concept":[{"code":"A","designation":[{"language":"ru","value":"Newer alias"}]}]})).await;
    for store in &stores {
        let app = api::create_router(store.clone());
        let (status, body) = request(
            &app,
            "GET",
            "/r4/ValueSet/$expand?url=urn:required:set",
            Value::Null,
        )
        .await;
        assert_eq!(status, 422, "{body}");
        let mut unversioned = definition.clone();
        unversioned["extension"][0]["valueCanonical"] = json!("urn:required:supplement");
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![
                json!({"name":"valueSet","resource":unversioned}),
                json!({"name":"includeDesignations","valueBoolean":true}),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            body["expansion"]["contains"][0]["designation"][0]["value"],
            "Newer alias"
        );
    }
    sqlx::query("UPDATE code_systems SET status='retired' WHERE url='urn:required:supplement'")
        .execute(&pool)
        .await
        .unwrap();
    for store in &stores {
        let app = api::create_router(store.clone());
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/ValueSet/{id}/$expand"),
            Value::Null,
        )
        .await;
        assert_eq!(
            status, 422,
            "Retired overlays cannot fulfill a required dependency: {body}"
        );
    }
    cleanup(admin, pool, schema).await;
}
