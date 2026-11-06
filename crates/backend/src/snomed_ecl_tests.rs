//! Generated ECL graphs verify the declared bounded subset, not clinical content coverage.
use super::*;
use term_squid_core::snomed::{INTERNATIONAL, SYSTEM};

const VERSION: &str = "http://snomed.info/sct/900000000000207008/version/20260201";
const NEXT: &str = "http://snomed.info/sct/900000000000207008/version/20260301";

async fn fixture(pool: &PgPool, version: &str, date: &str, base: u32) -> Uuid {
    let id = native_test_system(pool, version).await;
    sqlx::query("INSERT INTO snomed_releases VALUES($1,$2,$3,'900000000000509007','en',repeat('a',64),'{\"generatedFixture\":true}')")
        .bind(id).bind(INTERNATIONAL).bind(chrono::NaiveDate::parse_from_str(date,"%Y-%m-%d").unwrap()).execute(pool).await.unwrap();
    for (offset, name, inactive) in [
        (0, "Root", false),
        (10000, "Left", false),
        (20000, "Right", false),
        (30000, "Diamond", false),
        (40000, "Leaf", false),
        (50000, "Inactive", true),
        (55000, "Below inactive", false),
        (60000, "Disconnected", false),
        (70000, "Stated only", false),
        (80000, "Other relation", false),
        (90000, "Inactive edge", false),
    ] {
        sqlx::query(
            "INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind((base + offset).to_string())
        .bind(format!("Native {name} {}", base + offset))
        .bind(sqlx::types::Json(if inactive {
            json!([{"code":"inactive","valueBoolean":true}])
        } else {
            json!([])
        }))
        .execute(pool)
        .await
        .unwrap();
    }
    for (edge, child, parent, active, kind, characteristic) in [
        ("left", 10000, 0, true, "116680003", "900000000000011006"),
        ("right", 20000, 0, true, "116680003", "900000000000011006"),
        (
            "diamond-left",
            30000,
            10000,
            true,
            "116680003",
            "900000000000011006",
        ),
        (
            "diamond-right",
            30000,
            20000,
            true,
            "116680003",
            "900000000000011006",
        ),
        (
            "leaf",
            40000,
            30000,
            true,
            "116680003",
            "900000000000011006",
        ),
        (
            "inactive",
            50000,
            0,
            true,
            "116680003",
            "900000000000011006",
        ),
        (
            "below-inactive",
            55000,
            50000,
            true,
            "116680003",
            "900000000000011006",
        ),
        ("stated", 70000, 0, true, "116680003", "900000000000010007"),
        ("other", 80000, 0, true, "999999999", "900000000000011006"),
        (
            "inactive-edge",
            90000,
            0,
            false,
            "116680003",
            "900000000000011006",
        ),
    ] {
        sqlx::query("INSERT INTO snomed_relationships VALUES($1,$2,'2026-02-01',$5,'1',$3,$4,0,$6,$7,'900000000000451002')")
            .bind(id).bind(edge).bind((base+child).to_string()).bind((base+parent).to_string())
            .bind(active).bind(kind).bind(characteristic).execute(pool).await.unwrap();
    }
    id
}

fn selection(expression: &str) -> Value {
    json!({"system":SYSTEM,"version":VERSION,"filter":[{"property":"constraint","op":"=","value":expression}]})
}

fn definition(expression: &str) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":[selection(expression)]}})
}

fn parameters(fields: Vec<Value>) -> Value {
    json!({"resourceType":"Parameters","parameter":fields})
}

async fn expand(app: &axum::Router, set: &Value, extra: Vec<Value>) -> (u16, Value) {
    request(
        app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(set.clone(), extra),
    )
    .await
}

fn assert_page(body: &Value, total: usize, expected: &[&str]) {
    assert_eq!(body["expansion"]["total"], total, "{body}");
    assert_eq!(
        body["expansion"]["contains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected,
        "{body}"
    );
}

async fn assert_expansion(app: &axum::Router, expression: &str, expected: &[&str]) -> Value {
    let (status, body) = expand(app, &definition(expression), vec![]).await;
    assert_eq!(status, 200, "{expression}: {body}");
    assert_page(&body, expected.len(), expected);
    body
}

async fn validate(app: &axum::Router, set: &Value, code: &str) -> (u16, Value) {
    request(
        app,
        "POST",
        "/r4/ValueSet/$validate-code",
        inline_parameters(
            set.clone(),
            vec![
                json!({"name":"system","valueUri":SYSTEM}),
                json!({"name":"code","valueCode":code}),
            ],
        ),
    )
    .await
}

fn implicit(base: &str, expression: &str) -> String {
    // ECL is a URI segment, not form data. Literal '+' is already encoded as %2B.
    let encoded = form_urlencoded::byte_serialize(expression.as_bytes())
        .collect::<String>()
        .replace('+', "%20");
    format!("{base}?fhir_vs=ecl/{}", encoded)
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
async fn ecl_all_hierarchy_operators_preserve_direction_directness_and_active_intermediates() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (expression, expected) in [
        ("100000", vec!["100000"]),
        (
            "*",
            vec![
                "100000", "110000", "120000", "130000", "140000", "155000", "160000", "170000",
                "180000", "190000",
            ],
        ),
        ("<100000", vec!["110000", "120000", "130000", "140000"]),
        (
            "<<100000",
            vec!["100000", "110000", "120000", "130000", "140000"],
        ),
        ("<!100000", vec!["110000", "120000"]),
        ("<<!100000", vec!["100000", "110000", "120000"]),
        (">140000", vec!["100000", "110000", "120000", "130000"]),
        (
            ">>140000",
            vec!["100000", "110000", "120000", "130000", "140000"],
        ),
        (">!130000", vec!["110000", "120000"]),
        (">>!130000", vec!["110000", "120000", "130000"]),
        (">155000", vec![]),
        (">>155000", vec!["155000"]),
    ] {
        assert_expansion(&app, expression, &expected).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_boolean_sets_grouping_and_labels_do_not_change_source_displays() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (expression, expected) in [
        ("<100000 AND >140000", vec!["110000", "120000", "130000"]),
        ("<100000, >140000", vec!["110000", "120000", "130000"]),
        (
            "100000 OR 160000 OR 190000",
            vec!["100000", "160000", "190000"],
        ),
        ("<<100000 MINUS <<110000", vec!["100000", "120000"]),
        ("<<110000 MINUS <<100000", vec![]),
        (
            "(<<100000 MINUS <<110000) OR 160000",
            vec!["100000", "120000", "160000"],
        ),
        (
            "<100000 AND (>140000 OR 160000)",
            vec!["110000", "120000", "130000"],
        ),
        ("* MINUS *", vec![]),
        ("100000 or 160000", vec!["100000", "160000"]),
        ("(((110000)))", vec!["110000"]),
    ] {
        assert_expansion(&app, expression, &expected).await;
    }
    let body = assert_expansion(
        &app,
        "110000 | Not the source display; русский + ελληνικά OR < |",
        &["110000"],
    )
    .await;
    assert_eq!(
        body["expansion"]["contains"][0]["display"],
        "Native Left 110000"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_implicit_urls_decode_once_preserve_unicode_labels_and_match_inline_results() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let expression = "<<110000 | не источник A+B | OR 160000";
    let expected =
        assert_expansion(&app, expression, &["110000", "130000", "140000", "160000"]).await;
    for base in [SYSTEM, VERSION] {
        let url = implicit(base, expression);
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![json!({"name":"url","valueUri":url})]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            body["expansion"]["contains"],
            expected["expansion"]["contains"]
        );
        let mut query = form_urlencoded::Serializer::new(String::new());
        query.append_pair("url", &url);
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/ValueSet/$expand?{}", query.finish()),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_page(&body, 4, &["110000", "130000", "140000", "160000"]);
    }
    for raw in [
        "%",
        "%2",
        "%GG",
        "%FF",
        "%C3%28",
        "%00",
        "%253C100000",
        "100000+OR+160000",
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![
                json!({"name":"url","valueUri":format!("{SYSTEM}?fhir_vs=ecl/{raw}")}),
            ]),
        )
        .await;
        assert_eq!(status, 422, "{raw}: {body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_membership_checks_positive_negative_and_inactive_candidates_for_all_operator_families()
{
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (expression, members) in [
        (
            "<<100000",
            vec!["100000", "110000", "120000", "130000", "140000"],
        ),
        ("<!100000", vec!["110000", "120000"]),
        (">!130000", vec!["110000", "120000"]),
        ("<<100000 MINUS <<110000", vec!["100000", "120000"]),
        (
            "*",
            vec![
                "100000", "110000", "120000", "130000", "140000", "155000", "160000", "170000",
                "180000", "190000",
            ],
        ),
    ] {
        let set = definition(expression);
        for code in [
            "100000", "110000", "120000", "130000", "140000", "150000", "155000", "160000",
            "170000", "180000", "190000", "999999",
        ] {
            let (status, body) = validate(&app, &set, code).await;
            assert_eq!(status, 200, "{expression} {code}: {body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                members.contains(&code),
                "{expression} {code}: {body}"
            );
        }
    }
    let (status, body) = validate(&app, &definition("*"), "100000:200000=300000").await;
    assert_eq!(
        status, 422,
        "Scalar candidate input must not be reinterpreted as postcoordination: {body}"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_stored_instance_import_intersections_and_exclusions_keep_native_set_semantics() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    insert_set(
        &pool,
        Some("ecl-left"),
        json!({"include":[selection("<<100000")]}),
    )
    .await;
    insert_set(&pool,Some("ecl-right"),json!({"include":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"120000"},{"code":"130000"},{"code":"160000"}]}]})).await;
    let root = insert_set(
        &pool,
        Some("ecl-root"),
        json!({"include":[{"valueSet":["urn:set|ecl-left","urn:set|ecl-right"]}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for path in [
        format!("/r4/ValueSet/{root}/$expand"),
        "/r4/ValueSet/$expand?url=urn:set%7Cecl-root".into(),
    ] {
        let (status, body) = request(&app, "GET", &path, Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_page(&body, 2, &["120000", "130000"]);
    }
    let set = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:set|ecl-left"]}],"exclude":[selection("<<110000")]}});
    let (status, body) = expand(&app, &set, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 2, &["100000", "120000"]);
    for (code, valid) in [
        ("100000", true),
        ("110000", false),
        ("120000", true),
        ("130000", false),
    ] {
        let (status, body) = validate(&app, &set, code).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_mixed_finite_and_temporary_contained_sets_leave_no_persisted_context() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let source = json!({"resourceType":"CodeSystem","url":"urn:ecl:finite","version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"Finite A"}]});
    let contained = json!({"resourceType":"ValueSet","url":"urn:ecl:context","version":"1","status":"active","contained":[{"resourceType":"ValueSet","id":"child","compose":{"include":[selection("<!100000")]}}],"compose":{"include":[{"valueSet":["#child"]},{"system":"urn:ecl:finite","version":"1"}]}});
    let before:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concepts)").fetch_one(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        parameters(vec![
            json!({"name":"url","valueUri":"urn:ecl:context"}),
            json!({"name":"tx-resource","resource":source}),
            json!({"name":"tx-resource","resource":contained}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 3, &["110000", "120000", "A"]);
    assert_eq!(body["expansion"]["contains"][2]["system"], "urn:ecl:finite");
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=urn:ecl:context",
        Value::Null,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let after:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concepts)").fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_pages_filter_totals_and_zero_count_use_the_complete_expression_result() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let set = definition("<<100000");
    for (offset, count, expected) in [
        (0, 0, vec![]),
        (0, 2, vec!["100000", "110000"]),
        (2, 2, vec!["120000", "130000"]),
        (4, 2, vec!["140000"]),
        (5, 2, vec![]),
        (i64::MAX, 1, vec![]),
    ] {
        let (status, body) = expand(
            &app,
            &set,
            vec![
                json!({"name":"offset","valueInteger":offset}),
                json!({"name":"count","valueInteger":count}),
            ],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_page(&body, 5, &expected);
    }
    let (status, body) = expand(
        &app,
        &set,
        vec![
            json!({"name":"filter","valueString":"DIAMOND"}),
            json!({"name":"count","valueInteger":1}),
        ],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 1, &["130000"]);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_active_scope_does_not_change_legacy_implicit_whole_or_isa_membership() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for suffix in ["?fhir_vs", "?fhir_vs=isa/100000"] {
        for code in ["150000", "155000"] {
            let (status, body) = request(
                &app,
                "POST",
                "/r4/ValueSet/$validate-code",
                parameters(vec![
                    json!({"name":"url","valueUri":format!("{VERSION}{suffix}")}),
                    json!({"name":"system","valueUri":SYSTEM}),
                    json!({"name":"code","valueCode":code}),
                ]),
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], true, "{body}");
        }
    }
    for (expression, code, valid) in [
        ("*", "150000", false),
        ("*", "155000", true),
        ("<<100000", "150000", false),
        ("<<100000", "155000", false),
    ] {
        let (status, body) = validate(&app, &definition(expression), code).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_unknown_or_inactive_focus_is_not_hidden_by_early_matches_empty_sets_or_zero_pages() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for expression in [
        "999999",
        "<<999999",
        "* OR 999999",
        "* MINUS 999999",
        "150000",
        "<<150000",
        "* OR <<150000",
        "160000 AND <<999999",
    ] {
        for count in [0, 10] {
            let (status, body) = expand(
                &app,
                &definition(expression),
                vec![json!({"name":"count","valueInteger":count})],
            )
            .await;
            assert_eq!(status, 422, "{expression}: {body}");
            assert!(body.get("expansion").is_none());
        }
        let (status, body) = validate(&app, &definition(expression), "160000").await;
        assert_eq!(status, 422, "{expression}: {body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_rejects_unsupported_or_malformed_grammar_instead_of_partial_interpretation() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for expression in [
        "^100000",
        "<<100000 : 200000 = 300000",
        "100000:200000=300000",
        "(100000 OR 110000",
        "100000 OR",
        "AND 100000",
        "100000 OR160000",
        "100000 AND 110000 OR 120000",
        "100000 MINUS 110000 MINUS 120000",
        "<< (100000 OR 110000)",
        "<<*",
        "<*",
        "0",
        "0100000",
        "10000",
        "1234567890123456789",
        "110000 | unterminated",
        "110000 || trailing",
    ] {
        let (status, body) = expand(&app, &definition(expression), vec![]).await;
        assert_eq!(status, 422, "{expression}: {body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    let mut wrong_operator = definition("<<100000");
    wrong_operator["compose"]["include"][0]["filter"][0]["op"] = "in".into();
    let (status, body) = expand(&app, &wrong_operator, vec![]).await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_ast_and_aggregate_graph_admission_apply_even_when_no_members_would_be_returned() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let sixteen = vec!["<<100000"; 16].join(" OR ");
    assert_expansion(
        &app,
        &sixteen,
        &["100000", "110000", "120000", "130000", "140000"],
    )
    .await;
    for expression in [
        format!("{}100000", " ".repeat(4097)),
        format!("{}100000{}", "(".repeat(33), ")".repeat(33)),
        vec!["100000"; 129].join(" OR "),
        vec!["<<100000"; 17].join(" OR "),
    ] {
        let (status, body) = expand(
            &app,
            &definition(&expression),
            vec![json!({"name":"count","valueInteger":0})],
        )
        .await;
        assert_eq!(status, 422, "{body}");
    }
    let mut aggregate = definition(&sixteen);
    aggregate["compose"]["include"][0]["filter"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"concept","op":"is-a","value":"100000"}));
    let (status, body) = expand(
        &app,
        &aggregate,
        vec![json!({"name":"count","valueInteger":0})],
    )
    .await;
    assert_eq!(
        status, 422,
        "Legacy and ECL walks share the same operation budget: {body}"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_selected_release_and_historical_date_never_borrow_anchors_from_another_edition() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", 100000).await;
    fixture(&pool, NEXT, "2026-03-01", 200000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    assert_expansion(
        &app,
        "<<100000",
        &["100000", "110000", "120000", "130000", "140000"],
    )
    .await;
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        parameters(vec![
            json!({"name":"url","valueUri":implicit(SYSTEM,"<<100000")}),
        ]),
    )
    .await;
    assert_eq!(status,422,"Default implicit context must use current International, not an anchor from older context: {body}");
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        parameters(vec![
            json!({"name":"url","valueUri":implicit(NEXT,"<!200000")}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 2, &["210000", "220000"]);
    let mut dated = definition("<<100000");
    dated["compose"]["include"][0]
        .as_object_mut()
        .unwrap()
        .remove("version");
    let (status, body) = expand(
        &app,
        &dated,
        vec![json!({"name":"date","valueDateTime":"2026-02-15"})],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(
        &body,
        5,
        &["100000", "110000", "120000", "130000", "140000"],
    );
    let (status, body) = expand(
        &app,
        &dated,
        vec![json!({"name":"date","valueDateTime":"2026-03-15"})],
    )
    .await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_committed_graph_and_lifecycle_changes_are_fresh_with_retention_on_and_off() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let apps = [
        api::create_router(Arc::new(PostgresStore::new(pool.clone()))),
        api::create_router(Arc::new(PostgresStore::without_prepared_cache(
            pool.clone(),
        ))),
    ];
    for app in &apps {
        assert_expansion(app, "<100000", &["110000", "120000", "130000", "140000"]).await;
    }
    let edge="INSERT INTO snomed_relationships VALUES($1,'fresh-parent','2026-02-01',true,'1','160000','100000',0,'116680003','900000000000011006','900000000000451002')";
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(edge).bind(id).execute(&mut *tx).await.unwrap();
    for app in &apps {
        assert_expansion(app, "<100000", &["110000", "120000", "130000", "140000"]).await;
    }
    tx.rollback().await.unwrap();
    sqlx::query(edge).bind(id).execute(&pool).await.unwrap();
    for app in &apps {
        assert_expansion(
            app,
            "<100000",
            &["110000", "120000", "130000", "140000", "160000"],
        )
        .await;
    }
    sqlx::query("UPDATE concepts SET properties='[{\"code\":\"inactive\",\"valueBoolean\":true}]' WHERE code_system_id=$1 AND code='110000'").bind(id).execute(&pool).await.unwrap();
    for app in &apps {
        assert_expansion(app, "<100000", &["120000", "130000", "140000", "160000"]).await;
    }
    sqlx::query("UPDATE snomed_relationships SET active=false WHERE code_system_id=$1 AND id='diamond-right'").bind(id).execute(&pool).await.unwrap();
    for app in &apps {
        assert_expansion(app, "<100000", &["120000", "160000"]).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_diamond_and_projection_cycle_walks_terminate_without_duplicate_or_partial_members() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool, VERSION, "2026-02-01", 100000).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    assert_expansion(
        &app,
        "<<100000",
        &["100000", "110000", "120000", "130000", "140000"],
    )
    .await;
    // Importers reject cyclic source releases. A direct generated projection mutation
    // nevertheless must not turn a bounded provider walk into an infinite query.
    sqlx::query("INSERT INTO snomed_relationships VALUES($1,'cycle','2026-02-01',true,'1','100000','130000',0,'116680003','900000000000011006','900000000000451002')")
        .bind(id).execute(&pool).await.unwrap();
    assert_expansion(
        &app,
        "<<100000",
        &["100000", "110000", "120000", "130000", "140000"],
    )
    .await;
    assert_expansion(&app, ">100000", &["110000", "120000", "130000"]).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn ecl_membership_walk_limits_reject_early_positive_and_derived_false_results() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool, VERSION, "2026-02-01", 100000).await;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Long generated ancestry '||n,'[]' FROM generate_series(300000,308200)n")
        .bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships SELECT $1,'long-'||n,'2026-02-01'::date,true,'1',n::text,(n-1)::text,0,'116680003','900000000000011006','900000000000451002' FROM generate_series(300001,308200)n")
        .bind(id).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for expression in [
        "<<300000 OR 308200",
        "<<300000 AND 100000",
        "* MINUS <<300000",
    ] {
        let (status, body) = validate(&app, &definition(expression), "308200").await;
        assert_eq!(status,422,"No partial positive/negative may bypass the 8192-node membership walk: {expression}: {body}");
    }
    let (status, body) = validate(&app, &definition("<<300000"), "300001").await;
    assert_eq!(
        status, 200,
        "The rejected long request must not poison later bounded walks: {body}"
    );
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    cleanup(admin, pool, schema).await;
}
