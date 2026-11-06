//! Generated concept-only refsets exercise availability, not clinical/refset-field coverage.
use super::*;
use term_squid_core::snomed::{INTERNATIONAL, SYSTEM};

const VERSION: &str = "http://snomed.info/sct/900000000000207008/version/20260201";
const NEXT: &str = "http://snomed.info/sct/900000000000207008/version/20260301";
const REFSET: &str = "700001";
const EMPTY: &str = "700002";

async fn fixture(pool: &PgPool, version: &str, date: &str, loaded: bool) -> Uuid {
    let id = native_test_system(pool, version).await;
    let release = date.replace('-', "");
    let source = if loaded {
        json!({"file":format!("der2_Refset_SimpleSnapshot_GENERATED_{release}.txt"),"sha256":"a".repeat(64),"rows":5,"bytes":1024,"release":release,"completeSnapshot":true,"conceptOnly":true,"refsetIds":[REFSET,EMPTY]})
    } else {
        json!({})
    };
    sqlx::query("INSERT INTO snomed_releases(code_system_id,edition_module,release_date,dialect,language,fingerprint,provenance,simple_refsets_loaded,simple_refset_source) VALUES($1,$2,$3,'900000000000509007','en',repeat('a',64),'{\"generatedFixture\":true}',$4,$5)")
        .bind(id).bind(INTERNATIONAL).bind(chrono::NaiveDate::parse_from_str(date,"%Y-%m-%d").unwrap()).bind(loaded).bind(sqlx::types::Json(source)).execute(pool).await.unwrap();
    for (code, name, inactive) in [
        ("100000", "Root", false),
        ("110000", "Left", false),
        ("120000", "Right", false),
        ("130000", "Leaf", false),
        ("140000", "Inactive", true),
        ("150000", "Nonmember", false),
        (REFSET, "Declared refset", false),
        (EMPTY, "Known empty refset", false),
        ("700003", "Undeclared refset", false),
    ] {
        sqlx::query(
            "INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(code)
        .bind(format!("Generated {name}"))
        .bind(sqlx::types::Json(
            json!([{"code":"inactive","valueBoolean":inactive}]),
        ))
        .execute(pool)
        .await
        .unwrap();
    }
    for (edge, child, parent) in [
        ("left", "110000", "100000"),
        ("right", "120000", "100000"),
        ("leaf", "130000", "110000"),
        ("inactive", "140000", "100000"),
    ] {
        sqlx::query("INSERT INTO snomed_relationships VALUES($1,$2,$3,true,'1',$4,$5,0,'116680003','900000000000011006','900000000000451002')").bind(id).bind(edge).bind(chrono::NaiveDate::parse_from_str(date,"%Y-%m-%d").unwrap()).bind(child).bind(parent).execute(pool).await.unwrap();
    }
    if loaded {
        for refset in [REFSET, EMPTY] {
            sqlx::query("INSERT INTO snomed_simple_refsets VALUES($1,$2)")
                .bind(id)
                .bind(refset)
                .execute(pool)
                .await
                .unwrap();
        }
        for (code, active) in [
            ("110000", true),
            ("110000", true),
            ("130000", true),
            ("140000", true),
            ("120000", false),
        ] {
            sqlx::query("INSERT INTO snomed_simple_members VALUES($1,$2,$3,$4,'1',$5,$6,'{}')")
                .bind(id)
                .bind(Uuid::new_v4())
                .bind(chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap())
                .bind(active)
                .bind(REFSET)
                .bind(code)
                .execute(pool)
                .await
                .unwrap();
        }
    }
    id
}
fn selection(refset: &str) -> Value {
    json!({"system":SYSTEM,"version":VERSION,"filter":[{"property":"concept","op":"in","value":refset}]})
}
fn definition(refset: &str) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":[selection(refset)]}})
}
fn ecl(expression: &str) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":[{"system":SYSTEM,"version":VERSION,"filter":[{"property":"constraint","op":"=","value":expression}]}]}})
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
fn assert_page(body: &Value, total: usize, expected: &[&str]) {
    assert_eq!(body["expansion"]["total"], total, "{body}");
    assert_eq!(
        body["expansion"]["contains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|member| member["code"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected,
        "{body}"
    );
}
async fn check(app: &axum::Router, set: &Value, expected: &[&str]) {
    let (status, body) = expand(app, set, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, expected.len(), expected);
}
async fn cleanup(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
async fn implicit(
    app: &axum::Router,
    base: &str,
    refset: &str,
    code: Option<&str>,
) -> (u16, Value) {
    let mut fields =
        vec![json!({"name":"url","valueUri":format!("{base}?fhir_vs=refset/{refset}")})];
    if let Some(code) = code {
        fields.extend([
            json!({"name":"system","valueUri":SYSTEM}),
            json!({"name":"code","valueCode":code}),
        ]);
    }
    request(
        app,
        "POST",
        if code.is_some() {
            "/r4/ValueSet/$validate-code"
        } else {
            "/r4/ValueSet/$expand"
        },
        parameters(fields),
    )
    .await
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_primary_filter_deduplicates_active_rows_without_suppressing_inactive_concepts(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    check(&app, &definition(REFSET), &["110000", "130000", "140000"]).await;
    let (status, body) = expand(&app, &definition(REFSET), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["contains"][2]["inactive"], true);
    check(&app, &definition(EMPTY), &[]).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_membership_distinguishes_nonmembers_inactive_members_and_known_empty_sources(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (refset, code, expected) in [
        (REFSET, "110000", true),
        (REFSET, "130000", true),
        (REFSET, "140000", true),
        (REFSET, "120000", false),
        (REFSET, "150000", false),
        (REFSET, "999999", false),
        (EMPTY, "110000", false),
    ] {
        let (status, body) = validate(&app, &definition(refset), code).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            parameter(&body, "result")["valueBoolean"],
            expected,
            "{body}"
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_active_only_and_compose_inactive_are_explicit_not_implicit() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = expand(
        &app,
        &definition(REFSET),
        vec![json!({"name":"activeOnly","valueBoolean":true})],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 2, &["110000", "130000"]);
    let mut active = definition(REFSET);
    active["compose"]["inactive"] = false.into();
    check(&app, &active, &["110000", "130000"]).await;
    let (status, body) = validate(&app, &active, "140000").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    let mut all = definition(REFSET);
    all["compose"]["inactive"] = true.into();
    check(&app, &all, &["110000", "130000", "140000"]).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_implicit_post_get_and_stored_instance_match_explicit_membership() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for base in [SYSTEM, VERSION] {
        let (status, body) = implicit(&app, base, REFSET, None).await;
        assert_eq!(status, 200, "{body}");
        assert_page(&body, 3, &["110000", "130000", "140000"]);
        let (status, body) = implicit(&app, base, REFSET, Some("140000")).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    }
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("url", &format!("{VERSION}?fhir_vs=refset/{REFSET}"));
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/$expand?{}", query.finish()),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 3, &["110000", "130000", "140000"]);
    let stored = insert_set(
        &pool,
        Some("refset-instance"),
        definition(REFSET)["compose"].clone(),
    )
    .await;
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{stored}/$expand"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 3, &["110000", "130000", "140000"]);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_ecl_member_of_retains_active_substrate_and_ignores_labels() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for expression in ["^700001", "^ 700001 | Not the source label; μ |"] {
        check(&app, &ecl(expression), &["110000", "130000"]).await;
    }
    let (status, body) = validate(&app, &ecl("^700001"), "140000").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("url", &format!("{VERSION}?fhir_vs=ecl/%5E700001"));
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/$expand?{}", query.finish()),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 2, &["110000", "130000"]);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_ecl_boolean_hierarchy_and_empty_operands_preserve_set_semantics() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (expression, expected) in [
        ("^700001 AND <100000", vec!["110000", "130000"]),
        ("^700001 MINUS <110000", vec!["110000"]),
        ("^700001 OR 120000", vec!["110000", "120000", "130000"]),
        ("^700002 OR ^700001", vec!["110000", "130000"]),
        ("^700002 AND ^700001", vec![]),
        ("^700001 MINUS ^700002", vec!["110000", "130000"]),
        ("<100000 MINUS ^700001", vec!["120000"]),
        ("(^700001 AND >130000) OR 120000", vec!["110000", "120000"]),
    ] {
        let set = ecl(expression);
        check(&app, &set, &expected).await;
        for code in ["100000", "110000", "120000", "130000", "140000"] {
            let (status, body) = validate(&app, &set, code).await;
            assert_eq!(status, 200, "{expression}: {body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                expected.contains(&code),
                "{expression} {code}: {body}"
            );
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_pages_zero_count_and_text_filter_use_deduplicated_complete_totals() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (offset, count, expected) in [
        (0, 0, vec![]),
        (0, 1, vec!["110000"]),
        (1, 1, vec!["130000"]),
        (2, 2, vec!["140000"]),
        (3, 2, vec![]),
    ] {
        let (status, body) = expand(
            &app,
            &definition(REFSET),
            vec![
                json!({"name":"offset","valueInteger":offset}),
                json!({"name":"count","valueInteger":count}),
            ],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_page(&body, 3, &expected);
    }
    let (status, body) = expand(
        &app,
        &definition(REFSET),
        vec![json!({"name":"filter","valueString":"LEFT"})],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 1, &["110000"]);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_unavailable_file_is_not_false_empty_or_short_circuited() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", false).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for set in [
        definition(REFSET),
        ecl("^700001"),
        ecl("* OR ^700001"),
        ecl("100000 AND ^700001"),
        ecl("* MINUS ^700001"),
    ] {
        for count in [0, 5] {
            let (status, body) = expand(
                &app,
                &set,
                vec![json!({"name":"count","valueInteger":count})],
            )
            .await;
            assert_eq!(status, 422, "{body}");
            assert!(body.get("expansion").is_none());
        }
        let (status, body) = validate(&app, &set, "100000").await;
        assert_eq!(status, 422, "{body}");
    }
    for code in [None, Some("999999"), Some("110000")] {
        let (status, body) = implicit(&app, VERSION, REFSET, code).await;
        assert_eq!(status, 422, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_declaration_registry_source_list_and_active_identity_are_all_required() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for refset in ["700003", "799999"] {
        for set in [definition(refset), ecl(&format!("* OR ^{refset}"))] {
            let (status, body) =
                expand(&app, &set, vec![json!({"name":"count","valueInteger":0})]).await;
            assert_eq!(status, 422, "{body}");
            let (status, body) = validate(&app, &set, "110000").await;
            assert_eq!(status, 422, "{body}");
        }
    }
    sqlx::query("UPDATE snomed_releases SET simple_refset_source=jsonb_set(simple_refset_source,'{refsetIds}','[\"700002\"]') WHERE code_system_id=$1").bind(id).execute(&pool).await.unwrap();
    let (status, body) = expand(&app, &definition(REFSET), vec![]).await;
    assert_eq!(
        status, 422,
        "An undeclared registry row must not prove source availability: {body}"
    );
    sqlx::query("UPDATE snomed_releases SET simple_refset_source=jsonb_set(simple_refset_source,'{refsetIds}','[\"700001\",\"700002\"]') WHERE code_system_id=$1").bind(id).execute(&pool).await.unwrap();
    sqlx::query("UPDATE concepts SET properties='[{\"code\":\"inactive\",\"valueBoolean\":true}]' WHERE code_system_id=$1 AND code='700001'").bind(id).execute(&pool).await.unwrap();
    let (status, body) = expand(&app, &definition(REFSET), vec![]).await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_unsupported_global_nested_fields_and_history_grammars_are_explicit_errors()
{
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for expression in [
        "^*",
        "^(700001 OR 700002)",
        "^<700001",
        "^700001 {{+HISTORY}}",
        "^700001 : 110000=120000",
        "^700001 [referencedComponentId=110000]",
        "^700001 |unclosed",
        "^0700001",
    ] {
        let (status, body) = expand(&app, &ecl(expression), vec![]).await;
        assert_eq!(status, 422, "{expression}: {body}");
    }
    for suffix in [
        "?fhir_vs=refset",
        "?fhir_vs=refset/*",
        "?fhir_vs=refset/700001&extra=1",
        "?fhir_vs=refset/700001#fragment",
        "?fhir_vs=refset/700001/extra",
        "?fhir_vs=refset/%37%30%30%30%30%31",
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            parameters(vec![
                json!({"name":"url","valueUri":format!("{VERSION}{suffix}")}),
            ]),
        )
        .await;
        assert_eq!(status, 422, "{suffix}: {body}");
    }
    for value in ["700001,700002", "^700001", "*", "700001 |label|"] {
        let (status, body) = expand(&app, &definition(value), vec![]).await;
        assert_eq!(status, 422, "{value}: {body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_exact_default_and_temporal_versions_never_mix_membership_sources() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let next = fixture(&pool, NEXT, "2026-03-01", true).await;
    sqlx::query("UPDATE snomed_simple_members SET active=NOT active WHERE code_system_id=$1 AND referenced_component_id IN ('120000','130000')").bind(next).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    check(&app, &definition(REFSET), &["110000", "130000", "140000"]).await;
    let (status, body) = implicit(&app, SYSTEM, REFSET, None).await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 3, &["110000", "120000", "140000"]);
    assert!(body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .all(|member| member["version"] == NEXT));
    let mut dated = definition(REFSET);
    dated["compose"]["include"][0]
        .as_object_mut()
        .unwrap()
        .remove("version");
    for (date, expected) in [
        ("2026-02-15", vec!["110000", "130000", "140000"]),
        ("2026-03-15", vec!["110000", "120000", "140000"]),
    ] {
        let (status, body) = expand(
            &app,
            &dated,
            vec![json!({"name":"date","valueDateTime":date})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_page(&body, 3, &expected);
    }
    let mut pinned = definition(REFSET);
    pinned["compose"]["include"][0]["version"] = NEXT.into();
    check(&app, &pinned, &["110000", "120000", "140000"]).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_import_intersections_and_exclusions_preserve_inactive_members() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    insert_set(
        &pool,
        Some("refset-source"),
        definition(REFSET)["compose"].clone(),
    )
    .await;
    insert_set(&pool,Some("refset-selected"),json!({"include":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"130000"},{"code":"140000"}]}]})).await;
    let set = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:set|refset-source","urn:set|refset-selected"]}]}});
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    check(&app, &set, &["130000", "140000"]).await;
    let excluded = json!({"resourceType":"ValueSet","compose":{"include":[selection(REFSET)],"exclude":[{"valueSet":["urn:set|refset-selected"]}]}});
    check(&app, &excluded, &["110000"]).await;
    let undeclared = json!({"resourceType":"ValueSet","compose":{"include":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"110000"}]}],"exclude":[selection("700003")]}});
    let (status, body) = expand(&app, &undeclared, vec![]).await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_contained_and_request_local_finite_sources_do_not_persist_context() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, VERSION, "2026-02-01", true).await;
    let before:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concepts)").fetch_one(&pool).await.unwrap();
    let temporary = json!({"resourceType":"CodeSystem","url":"urn:refset:finite","version":"1","status":"active","content":"complete","concept":[{"code":"A","display":"Temporary A"}]});
    let set = json!({"resourceType":"ValueSet","url":"urn:refset:context","contained":[{"resourceType":"ValueSet","id":"members","compose":{"include":[selection(REFSET)]}}],"compose":{"include":[{"valueSet":["#members"]},{"system":"urn:refset:finite","version":"1"}]}});
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        parameters(vec![
            json!({"name":"valueSet","resource":set}),
            json!({"name":"tx-resource","resource":temporary}),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_page(&body, 4, &["110000", "130000", "140000", "A"]);
    assert_eq!(
        body["expansion"]["contains"][3]["system"],
        "urn:refset:finite"
    );
    let after:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concepts)").fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_commits_rollbacks_registry_and_nonconcept_rows_are_fresh_with_both_cache_modes(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool, VERSION, "2026-02-01", true).await;
    let apps = [
        api::create_router(Arc::new(PostgresStore::new(pool.clone()))),
        api::create_router(Arc::new(PostgresStore::without_prepared_cache(
            pool.clone(),
        ))),
    ];
    for app in &apps {
        check(app, &definition(REFSET), &["110000", "130000", "140000"]).await;
    }
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE snomed_simple_members SET active=true WHERE code_system_id=$1 AND referenced_component_id='120000'").bind(id).execute(&mut *tx).await.unwrap();
    for app in &apps {
        check(app, &definition(REFSET), &["110000", "130000", "140000"]).await;
    }
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE snomed_simple_members SET active=true WHERE code_system_id=$1 AND referenced_component_id='120000'").bind(id).execute(&pool).await.unwrap();
    for app in &apps {
        check(
            app,
            &definition(REFSET),
            &["110000", "120000", "130000", "140000"],
        )
        .await;
    }
    sqlx::query(
        "INSERT INTO snomed_simple_members VALUES($1,$2,'2026-02-01',true,'1',$3,'999001','{}')",
    )
    .bind(id)
    .bind(Uuid::new_v4())
    .bind(REFSET)
    .execute(&pool)
    .await
    .unwrap();
    for app in &apps {
        check(
            app,
            &definition(REFSET),
            &["110000", "120000", "130000", "140000"],
        )
        .await;
        let (status, body) = validate(app, &definition(REFSET), "999001").await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    }
    sqlx::query("DELETE FROM snomed_simple_refsets WHERE code_system_id=$1 AND refset_id=$2")
        .bind(id)
        .bind(REFSET)
        .execute(&pool)
        .await
        .unwrap();
    for app in &apps {
        let (status, body) = validate(app, &definition(REFSET), "110000").await;
        assert_eq!(status, 422, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concept_refset_finite_snomed_payloads_never_reinterpret_refset_ids_as_literal_members() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let source = json!({"resourceType":"CodeSystem","url":SYSTEM,"version":VERSION,"status":"active","content":"complete","caseSensitive":true,"concept":[{"code":REFSET,"display":"Finite refset identity, not membership knowledge"}]});
    insert_system(&pool, source.clone()).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let ordinary = json!({"resourceType":"ValueSet","compose":{"include":[{"system":SYSTEM,"version":VERSION,"filter":[{"property":"code","op":"=","value":REFSET}]}]}});
    check(&app, &ordinary, &[REFSET]).await;
    for set in [definition(REFSET), ecl("^700001")] {
        for count in [0, 5] {
            let (status, body) = expand(
                &app,
                &set,
                vec![json!({"name":"count","valueInteger":count})],
            )
            .await;
            assert_eq!(
                status, 422,
                "A finite SNOMED payload supplies no complete refset source: {body}"
            );
        }
        let (status, body) = validate(&app, &set, REFSET).await;
        assert_eq!(
            status, 422,
            "Refset ID must not become a literal-code member: {body}"
        );
    }
    for code in [None, Some(REFSET)] {
        let (status, body) = implicit(&app, VERSION, REFSET, code).await;
        assert_eq!(status, 422, "{body}");
    }
    // Existing request-local safety forbids overriding an authoritative system,
    // even with a finite resource claiming the refset concept itself.
    let mut fields = inline_parameters(definition(REFSET), vec![]);
    fields["parameter"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"tx-resource","resource":source}));
    let (status, body) = request(&app, "POST", "/r4/ValueSet/$expand", fields).await;
    assert_eq!(
        status, 400,
        "Authoritative tx-resource overrides are prohibited: {body}"
    );
    cleanup(admin, pool, schema).await;
}
