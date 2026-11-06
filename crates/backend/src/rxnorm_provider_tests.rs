//! Generated active RxNorm projections test provider boundaries, not clinical completeness.
#[path = "rxnorm_auxiliary_tests.rs"]
mod auxiliary;
#[path = "rxnorm_relationship_tests.rs"]
mod relationships;
use super::*;
use term_squid_core::rxnorm::{SYSTEM, VALUE_SET};

const VERSION: &str = "03022026";

async fn release(pool: &PgPool, version: &str, date: &str) -> Uuid {
    let id = Uuid::new_v4();
    let source = json!({"resourceType":"CodeSystem","url":SYSTEM,"version":version,"status":"active","name":"GeneratedRxNorm","date":date,"content":"not-present","caseSensitive":true,"property":[{"code":"inactive","type":"boolean"},{"code":"TTY","type":"code"},{"code":"SAB","type":"code"},{"code":"preferredTTY","type":"code"},{"code":"rxnorm-status","type":"code"},{"code":"source-table","type":"string"}]});
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,$2,$3,'active','GeneratedRxNorm',$4,'rxnorm-rrf')")
        .bind(id).bind(SYSTEM).bind(version).bind(sqlx::types::Json(source)).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO rxnorm_releases(code_system_id,version,release_date,fingerprint,provenance) VALUES($1,$2,$3,repeat('a',64),'{\"generatedFixture\":true}')")
        .bind(id).bind(version).bind(chrono::NaiveDate::parse_from_str(date,"%Y-%m-%d").unwrap()).execute(pool).await.unwrap();
    for (code, display, tty) in [
        ("111", "Generated ingredient", "IN"),
        ("222", "Generated tablet", "SCD"),
    ] {
        let properties = json!([{"code":"inactive","valueBoolean":false},{"code":"source-table","valueString":"RXNCONSO.RRF"},{"code":"rxnorm-status","valueCode":"active"},{"code":"preferredTTY","valueCode":tty},{"code":"TTY","valueCode":tty},{"code":"SAB","valueCode":"RXNORM"},{"code":"SAB","valueCode":if code=="111" {"VANDF"} else {"MTHSPL"}}]);
        let designations = if code == "222" {
            json!([{"language":"en","use":{"system":SYSTEM,"code":"SY"},"value":"Generated synonym"},{"language":"en","use":{"system":SYSTEM,"code":"PSN"},"value":"Generated prescribing label"}])
        } else {
            json!([])
        };
        sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties,designations) VALUES($1,$2,$3,$4,$5)")
            .bind(id).bind(code).bind(display).bind(sqlx::types::Json(properties)).bind(sqlx::types::Json(designations)).execute(pool).await.unwrap();
        atom(pool, id, code, code, "RXNORM", tty, display, "N").await;
    }
    for (aui, code, sab, tty, term, suppress) in [
        ("301", "222", "RXNORM", "SY", "Generated synonym", "N"),
        (
            "302",
            "222",
            "RXNORM",
            "PSN",
            "Generated prescribing label",
            "N",
        ),
        ("303", "222", "RXNORM", "SY", "Generated synonym", "N"),
        ("304", "222", "RXNORM", "SY", "Obsolete alias", "O"),
        ("305", "111", "VANDF", "SCD", "Foreign source label", "N"),
        ("307", "222", "MTHSPL", "SCD", "Foreign tablet label", "N"),
        (
            "306",
            "300",
            "RXNORM",
            "SCD",
            "Obsolete generated concept",
            "O",
        ),
    ] {
        atom(pool, id, aui, code, sab, tty, term, suppress).await;
    }
    id
}

#[allow(clippy::too_many_arguments)]
async fn atom(
    pool: &PgPool,
    id: Uuid,
    aui: &str,
    code: &str,
    sab: &str,
    tty: &str,
    term: &str,
    suppress: &str,
) {
    let fields = json!({"RXCUI":code,"LAT":"ENG","TS":"","LUI":"","STT":"","SUI":"","ISPREF":"","RXAUI":aui,"SAUI":"","SCUI":"","SDUI":"","SAB":sab,"TTY":tty,"CODE":code,"STR":term,"SRL":"","SUPPRESS":suppress,"CVF":""});
    sqlx::query("INSERT INTO rxnorm_atoms(code_system_id,rxaui,rxcui,lat,sab,tty,term,suppress,fields) VALUES($1,$2,$3,'ENG',$4,$5,$6,$7,$8)")
        .bind(id).bind(aui).bind(code).bind(sab).bind(tty).bind(term).bind(suppress).bind(sqlx::types::Json(fields)).execute(pool).await.unwrap();
}

fn set(selection: Value) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":[selection]}})
}
fn active() -> Value {
    json!({"system":SYSTEM,"version":VERSION,"filter":[{"property":"rxnorm-status","op":"=","value":"active"}]})
}
async fn expand(app: &axum::Router, resource: Value, extra: Vec<Value>) -> (u16, Value) {
    request(
        app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(resource, extra),
    )
    .await
}
async fn member(app: &axum::Router, resource: Value, code: &str) -> (u16, Value) {
    request(
        app,
        "POST",
        "/r4/ValueSet/$validate-code",
        inline_parameters(
            resource,
            vec![
                json!({"name":"system","valueUri":SYSTEM}),
                json!({"name":"code","valueCode":code}),
            ],
        ),
    )
    .await
}
fn codes(body: &Value) -> Vec<&str> {
    body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["code"].as_str().unwrap())
        .collect()
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
async fn rxnorm_point_reads_keep_active_scope_partial_knowledge_and_source_identity() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    for cached in [true, false] {
        let store = if cached {
            PostgresStore::new(pool.clone())
        } else {
            PostgresStore::without_prepared_cache(pool.clone())
        };
        let app = api::create_router(Arc::new(store));
        for path in [
            format!("/r4/CodeSystem/$lookup?system={SYSTEM}&version={VERSION}&code=222"),
            format!("/r4/CodeSystem/{id}/$lookup?code=222"),
        ] {
            let (status, body) = request(&app, "GET", &path, Value::Null).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(
                parameter(&body, "display")["valueString"],
                "Generated tablet"
            );
            assert_eq!(parameter(&body, "version")["valueString"], VERSION);
            assert!(body["parameter"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["name"] == "property"
                    && p["part"].as_array().is_some_and(|parts| parts
                        .iter()
                        .any(
                            |part| part["name"] == "code" && part["valueCode"] == "preferredTTY"
                        )
                        && parts.iter().any(|part| part["valueCode"] == "SCD"))));
        }
        for code in ["300", "999999", "RX222", "0222"] {
            for operation in ["$lookup", "$validate-code"] {
                let (status, body) = request(
                    &app,
                    "GET",
                    &format!(
                        "/r4/CodeSystem/{operation}?{}={SYSTEM}&version={VERSION}&code={code}",
                        if operation == "$lookup" {
                            "system"
                        } else {
                            "url"
                        }
                    ),
                    Value::Null,
                )
                .await;
                assert_eq!(status, 422, "{code}: {body}");
                assert_eq!(body["resourceType"], "OperationOutcome");
            }
        }
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&version=01012000&code=222"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 404, "{body}");
        let (status, body) = request(
            &app,
            "GET",
            &format!(
                "/r4/CodeSystem/$subsumes?system={SYSTEM}&version={VERSION}&codeA=111&codeB=222"
            ),
            Value::Null,
        )
        .await;
        assert_eq!(status, 400, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_expansion_requires_known_active_scope_and_never_enumerates_missing_history() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    release(&pool, VERSION, "2026-03-02").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let whole = set(json!({"system":SYSTEM,"version":VERSION}));
    for resource in [
        whole.clone(),
        set(json!({"system":SYSTEM,"filter":[{"property":"TTY","op":"=","value":"SCD"}]})),
    ] {
        let (status, body) = expand(&app, resource, vec![]).await;
        assert_eq!(status, 422, "{body}");
    }
    for (resource, extra) in [
        (set(active()), vec![]),
        (
            whole.clone(),
            vec![json!({"name":"activeOnly","valueBoolean":true})],
        ),
        (
            json!({"resourceType":"ValueSet","compose":{"inactive":false,"include":[{"system":SYSTEM,"version":VERSION}]}}),
            vec![],
        ),
    ] {
        let (status, body) = expand(&app, resource, extra).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), ["111", "222"]);
        assert_eq!(body["expansion"]["total"], 2);
    }
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/$expand?url={VALUE_SET}&activeOnly=true"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["111", "222"]);
    for (code, expected) in [("111", 200), ("300", 422), ("999999", 422)] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/ValueSet/$validate-code?url={VALUE_SET}&system={SYSTEM}&code={code}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, expected, "{body}");
        if status == 200 {
            assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        }
    }
    let explicit = set(json!({"system":SYSTEM,"concept":[{"code":"111"}]}));
    let (status, body) = member(&app, explicit, "222").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    let (status, body) = expand(
        &app,
        set(json!({"system":SYSTEM,"concept":[{"code":"300"}]})),
        vec![],
    )
    .await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_tty_and_source_filters_use_real_atom_scope_and_preserve_pages() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    release(&pool, VERSION, "2026-03-02").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (property, op, value, expected) in [
        ("TTY", "=", "SCD", vec!["222"]),
        ("TTY", "=", "PSN", vec!["222"]),
        ("TTY", "=", "SY", vec![]),
        ("SAB", "=", "VANDF", vec!["111"]),
        ("SAB", "in", "VANDF,MTHSPL", vec!["111", "222"]),
        ("TTY", "in", "IN,SCD", vec!["111", "222"]),
    ] {
        let mut selection = active();
        selection["filter"]
            .as_array_mut()
            .unwrap()
            .push(json!({"property":property,"op":op,"value":value}));
        let (status, body) = expand(&app, set(selection.clone()), vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), expected);
        assert_eq!(body["expansion"]["total"], expected.len());
        let (status, body) = expand(
            &app,
            set(selection),
            vec![json!({"name":"count","valueInteger":0})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body).len(), 0);
        assert_eq!(body["expansion"]["total"], expected.len());
    }
    for (property, op, value) in [
        ("STY", "=", "T109"),
        ("RO", "=", "CUI:111"),
        ("TTY", "regex", ".*"),
        ("inactive", "=", "true"),
    ] {
        let mut selection = active();
        selection["filter"]
            .as_array_mut()
            .unwrap()
            .push(json!({"property":property,"op":op,"value":value}));
        let (status, body) = expand(&app, set(selection), vec![]).await;
        assert_eq!(status, 422, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_alias_search_and_display_validation_preserve_normalized_primary_names() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    release(&pool, VERSION, "2026-03-02").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (filter, expected) in [
        ("SYNONYM", vec!["222"]),
        ("Obsolete alias", vec![]),
        ("Foreign source label", vec![]),
        ("222", vec!["222"]),
    ] {
        let (status, body) = expand(
            &app,
            set(active()),
            vec![json!({"name":"filter","valueString":filter})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), expected);
        assert_eq!(body["expansion"]["total"], expected.len());
        if !expected.is_empty() {
            assert_eq!(
                body["expansion"]["contains"][0]["display"],
                "Generated tablet"
            );
        }
    }
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&code=222&property=designation"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(body.to_string().contains("Generated prescribing label"));
    assert!(!body.to_string().contains("Foreign source label"));
    for display in ["GENERATED TABLET", "generated synonym"] {
        for target in ["CodeSystem", "ValueSet"] {
            let mut parameters = vec![
                json!({"name":"code","valueCode":"222"}),
                json!({"name":"display","valueString":display}),
            ];
            if target == "CodeSystem" {
                parameters.push(json!({"name":"url","valueUri":SYSTEM}));
            } else {
                parameters.push(json!({"name":"system","valueUri":SYSTEM}));
                parameters.push(json!({"name":"valueSet","resource":set(active())}));
            }
            let (status, body) = request(
                &app,
                "POST",
                &format!("/r4/{target}/$validate-code"),
                json!({"resourceType":"Parameters","parameter":parameters}),
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
            assert_eq!(
                parameter(&body, "display")["valueString"],
                "Generated tablet"
            );
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_versions_date_activation_rollback_and_cross_process_freshness_remain_exact() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = release(&pool, "02022026", "2026-02-02").await;
    sqlx::query("UPDATE concepts SET display='Older generated tablet' WHERE code_system_id=$1 AND code='222'").bind(old).execute(&pool).await.unwrap();
    let current = release(&pool, VERSION, "2026-03-02").await;
    sqlx::query("UPDATE code_systems SET updated_at='2099-01-01' WHERE id=$1")
        .bind(current)
        .execute(&pool)
        .await
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (extra, expected_version, expected_display) in [
        ("", VERSION, "Generated tablet"),
        ("&version=02022026", "02022026", "Older generated tablet"),
        ("&date=2026-02-10", "02022026", "Older generated tablet"),
    ] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&code=222{extra}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "version")["valueString"], expected_version);
        assert_eq!(parameter(&body, "display")["valueString"], expected_display);
    }
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE concepts SET display='Rolled back' WHERE code_system_id=$1 AND code='222'")
        .bind(current)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&code=222"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        parameter(&body, "display")["valueString"],
        "Generated tablet"
    );
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE concepts SET display='Committed replacement' WHERE code_system_id=$1 AND code='222'").bind(current).execute(&pool).await.unwrap();
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&code=222"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        parameter(&body, "display")["valueString"],
        "Committed replacement"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_mixed_finite_sets_supplements_and_authoritative_temporary_guard() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    release(&pool, VERSION, "2026-03-02").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let finite = json!({"resourceType":"CodeSystem","url":"urn:rxnorm:finite","version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"Finite A"}]});
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[active(),{"system":"urn:rxnorm:finite","version":"1"}],"exclude":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"111"}]}]}});
    let(status,body)=request(&app,"POST","/r4/ValueSet/$expand",json!({"resourceType":"Parameters","parameter":[{"name":"valueSet","resource":resource},{"name":"tx-resource","resource":finite}]})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["222", "A"]);
    assert_eq!(body["expansion"]["total"], 2);
    let overlay = json!({"resourceType":"CodeSystem","url":"urn:rxnorm:supplement","version":"1","status":"active","content":"supplement","supplements":format!("{SYSTEM}|{VERSION}"),"concept":[{"code":"222","designation":[{"language":"de","value":"Generated German label"}]}]});
    let inspection = term_squid_ingestion::inspect(&overlay).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &overlay, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (status, body) = expand(
        &app,
        set(active()),
        vec![
            json!({"name":"displayLanguage","valueCode":"de"}),
            json!({"name":"includeDesignations","valueBoolean":true}),
        ],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let tablet = body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == "222")
        .unwrap();
    assert_eq!(tablet["display"], "Generated German label");
    assert!(tablet["designation"]
        .to_string()
        .contains("Generated synonym"));
    let fake = json!({"resourceType":"CodeSystem","url":SYSTEM,"version":VERSION,"status":"active","content":"complete","concept":[{"code":"fake"}]});
    let(status,body)=request(&app,"POST","/r4/CodeSystem/$lookup",json!({"resourceType":"Parameters","parameter":[{"name":"system","valueUri":SYSTEM},{"name":"code","valueCode":"fake"},{"name":"tx-resource","resource":fake}]})).await;
    assert_eq!(status, 400, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn rxnorm_required_supplements_label_bounds_and_object_alternatives_are_not_bypassed() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, VERSION, "2026-03-02").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut resource = set(active());
    resource["extension"] = json!([{"url":"http://hl7.org/fhir/StructureDefinition/valueset-supplement","valueCanonical":"urn:rxnorm:required"}]);
    let (status, body) = expand(&app, resource.clone(), vec![]).await;
    assert_eq!(status, 422, "{body}");
    let overlay = json!({"resourceType":"CodeSystem","url":"urn:rxnorm:required","version":"1","status":"active","content":"supplement","supplements":format!("{SYSTEM}|{VERSION}"),"concept":[{"code":"222","designation":[{"language":"de","value":"Required German label"}]}]});
    let inspection = term_squid_ingestion::inspect(&overlay).unwrap();
    let mut tx = pool.begin().await.unwrap();
    term_squid_ingestion::import_resource(&mut tx, &overlay, &inspection)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (status, body) = expand(&app, resource.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert!(body["expansion"]["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "used-supplement" && p["valueUri"] == "urn:rxnorm:required|1"));
    let (status, body) = request(&app,"POST","/r4/ValueSet/$validate-code",json!({"resourceType":"Parameters","parameter":[{"name":"valueSet","resource":resource.clone()},{"name":"codeableConcept","valueCodeableConcept":{"coding":[{"system":SYSTEM,"code":"999999"},{"system":SYSTEM,"code":"222","display":"GENERATED SYNONYM"}]}}]})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
    sqlx::query("UPDATE concepts SET designations=$2 WHERE code_system_id=$1 AND code='222'")
        .bind(id)
        .bind(sqlx::types::Json(json!((0..257)
            .map(|n| json!({"language":"en","value":format!("Generated label {n}")}))
            .collect::<Vec<_>>())))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&code=222&property=designation"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    let (status, body) = expand(
        &app,
        resource,
        vec![json!({"name":"includeDesignations","valueBoolean":true})],
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(body.get("expansion").is_none());
    cleanup(admin, pool, schema).await;
}
