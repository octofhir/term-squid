//! Generated primary links test source-scoped coded axes, not a complete Part registry.
use super::*;
use term_squid_core::loinc::PrimaryAxis;

const SYSTEM: &str = "http://loinc.org";
const CODES: [&str; 4] = ["1000-1", "1001-9", "1002-7", "1003-5"];
fn part(axis: PrimaryAxis, index: usize) -> String {
    let axis = PrimaryAxis::all().iter().position(|a| *a == axis).unwrap();
    format!("LP{}-0", 100 + axis + if index < 2 { 0 } else { 100 })
}
fn descriptor(version: &str) -> Value {
    json!({"file":"LoincPartLink_Primary.csv","sha256":"a".repeat(64),"rows":22,"bytes":4096,"version":version,"completeFile":true})
}
async fn link(pool: &PgPool, id: Uuid, row: i64, code: &str, axis: PrimaryAxis, value: &str) {
    sqlx::query("INSERT INTO loinc_primary_links(code_system_id,source_row,loinc_number,long_common_name,part_number,part_name,part_code_system,part_type_name,link_type_name,property,fields) VALUES($1,$2,$3,'Generated name',$4,'Generated Part','http://loinc.org',$5,'Primary',$6,'{}')")
        .bind(id).bind(row).bind(code).bind(value).bind(axis.part_type()).bind(axis.uri()).execute(pool).await.unwrap();
}
async fn fixture(pool: &PgPool, version: &str, loaded: bool) -> Uuid {
    let id = Uuid::new_v4();
    let mut properties = vec![json!({"code":"raw-COMPONENT","type":"string"})];
    if loaded {
        properties.extend(
            PrimaryAxis::all().map(|a| json!({"code":a.code(),"uri":a.uri(),"type":"code"})),
        );
    }
    let resource = json!({"resourceType":"CodeSystem","url":SYSTEM,"version":version,"content":"not-present","caseSensitive":false,"property":properties});
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider,updated_at) VALUES($1,$2,$3,'active','Generated LOINC',$4,'loinc-csv',CASE WHEN $3='2.81' THEN '2026-01-01'::timestamptz ELSE '2026-02-01'::timestamptz END)")
        .bind(id).bind(SYSTEM).bind(version).bind(sqlx::types::Json(resource)).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases(code_system_id,version,fingerprint,provenance,primary_links_loaded,primary_link_source) VALUES($1,$2,repeat('a',64),'{}',$3,$4)")
        .bind(id).bind(version).bind(loaded).bind(sqlx::types::Json(if loaded {descriptor(version)} else {json!({})})).execute(pool).await.unwrap();
    let mut row = 0;
    for (index, code) in CODES.into_iter().enumerate() {
        let mut properties = vec![
            json!({"code":"source-table","valueString":"Loinc.csv"}),
            json!({"code":"inactive","valueBoolean":index==3}),
        ];
        for axis in PrimaryAxis::all() {
            if axis == PrimaryAxis::Method && index % 2 == 0 {
                continue;
            }
            properties.push(json!({"code":format!("raw-{}",axis.code()),"valueString":format!("Raw {}",axis.code())}));
            if loaded {
                row += 1;
                let value = part(axis, index);
                link(pool, id, row, code, axis, &value).await;
                properties.push(json!({"code":axis.code(),"valueCode":value}));
            }
        }
        sqlx::query(
            "INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(code)
        .bind(format!("Generated {version} {index}"))
        .bind(sqlx::types::Json(properties))
        .execute(pool)
        .await
        .unwrap();
    }
    id
}
fn selected(version: &str, filters: Vec<Value>) -> Value {
    let mut all = vec![json!({"property":"source-table","op":"=","value":"Loinc.csv"})];
    all.extend(filters);
    json!({"system":SYSTEM,"version":version,"filter":all})
}
fn filter(axis: PrimaryAxis, op: &str, value: &str) -> Value {
    json!({"property":axis.code(),"op":op,"value":value})
}
fn set(selection: Value) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":[selection]}})
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
async fn lookup(app: &axum::Router, code: &str, options: &str) -> (u16, Value) {
    request(
        app,
        "GET",
        &format!("/r4/CodeSystem/$lookup?system={SYSTEM}&code={code}{options}"),
        Value::Null,
    )
    .await
}
fn codes(body: &Value) -> Vec<&str> {
    body["expansion"]["contains"]
        .as_array()
        .map(|a| a.iter().map(|c| c["code"].as_str().unwrap()).collect())
        .unwrap_or_default()
}
fn values<'a>(body: &'a Value, code: &str, kind: &str) -> Vec<&'a str> {
    body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["name"] == "property")
        .filter_map(|p| {
            let parts = p["part"].as_array()?;
            (parts
                .iter()
                .any(|p| p["name"] == "code" && p["valueCode"] == code))
            .then(|| {
                parts
                    .iter()
                    .find(|p| p["name"] == "value")
                    .and_then(|p| p[kind].as_str())
            })
            .flatten()
        })
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
async fn loinc_parts_lookup_returns_all_six_codes_without_replacing_raw_labels() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for options in ["", "&property=*"] {
        let (status, body) = lookup(&app, CODES[1], options).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "version")["valueString"], "2.82");
        for axis in PrimaryAxis::all() {
            assert_eq!(values(&body, axis.code(), "valueCode"), [part(axis, 1)]);
            assert_eq!(
                values(&body, &format!("raw-{}", axis.code()), "valueString"),
                [format!("Raw {}", axis.code())]
            );
            assert!(values(&body, axis.code(), "valueString").is_empty());
        }
    }
    for axis in PrimaryAxis::all() {
        let (status, body) = lookup(&app, CODES[1], &format!("&property={}", axis.code())).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(values(&body, axis.code(), "valueCode"), [part(axis, 1)]);
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_all_axis_equalities_and_bounded_in_are_literal_relations() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for axis in PrimaryAxis::all() {
        for op in ["=", "in"] {
            let mut literal = part(axis, 1).replacen("LP", "lp", 1);
            if op == "in" {
                literal.push_str(",LP99999999-9");
            }
            let resource = set(selected("2.82", vec![filter(axis, op, &literal)]));
            let (status, body) = expand(&app, resource.clone(), vec![]).await;
            assert_eq!(status, 200, "{axis:?} {op}: {body}");
            assert_eq!(
                codes(&body),
                if axis == PrimaryAxis::Method {
                    vec![CODES[1]]
                } else {
                    vec![CODES[0], CODES[1]]
                }
            );
            let (status, body) = member(&app, resource, CODES[1]).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "result")["valueBoolean"], true);
        }
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_exists_distinguishes_optional_method_and_unknown_part_literals() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (present, expected) in [
        ("true", vec![CODES[1], CODES[3]]),
        ("false", vec![CODES[0], CODES[2]]),
    ] {
        let (status, body) = expand(
            &app,
            set(selected(
                "2.82",
                vec![filter(PrimaryAxis::Method, "exists", present)],
            )),
            vec![],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), expected);
    }
    let (status, body) = lookup(&app, CODES[0], "&property=METHOD_TYP").await;
    assert_eq!(status, 200, "{body}");
    assert!(values(&body, "METHOD_TYP", "valueCode").is_empty());
    let (status, body) = expand(
        &app,
        set(selected(
            "2.82",
            vec![filter(PrimaryAxis::Component, "=", "LP99999999-9")],
        )),
        vec![],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    let (status, body) = member(
        &app,
        set(selected(
            "2.82",
            vec![filter(PrimaryAxis::Component, "=", "LP99999999-9")],
        )),
        CODES[0],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_missing_source_never_means_absent_property() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82", false).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for axis in PrimaryAxis::all() {
        let (status, body) = lookup(&app, CODES[0], &format!("&property={}", axis.code())).await;
        assert_eq!(status, 422, "{body}");
        for (op, value) in [
            ("=", "LP100-0"),
            ("in", "LP100-0,LP999-9"),
            ("exists", "true"),
            ("exists", "false"),
        ] {
            let resource = set(selected("2.82", vec![filter(axis, op, value)]));
            for extra in [vec![], vec![json!({"name":"count","valueInteger":0})]] {
                let (status, body) = expand(&app, resource.clone(), extra).await;
                assert_eq!(status, 422, "{body}");
            }
            let (status, body) = member(&app, resource, CODES[0]).await;
            assert_eq!(status, 422, "{body}");
        }
    }
    for options in ["", "&property=*", "&property=raw-COMPONENT"] {
        let (status, body) = lookup(&app, CODES[0], options).await;
        assert_eq!(status, 200, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_missing_source_is_guarded_in_excludes_imports_and_early_positive() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82", false).await;
    let missing = selected("2.82", vec![filter(PrimaryAxis::Method, "exists", "false")]);
    let explicit = json!({"system":SYSTEM,"version":"2.82","concept":[{"code":CODES[0]}]});
    let imported = json!({"resourceType":"ValueSet","url":"urn:loinc:missing-parts","compose":{"include":[missing.clone()]}});
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO value_sets(id,url,status,content) VALUES($1,'urn:loinc:missing-parts','active',$2)").bind(id).bind(sqlx::types::Json(imported)).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for resource in [
        json!({"resourceType":"ValueSet","compose":{"include":[explicit.clone(),missing.clone()]}}),
        json!({"resourceType":"ValueSet","compose":{"include":[explicit.clone()],"exclude":[missing]}}),
        json!({"resourceType":"ValueSet","compose":{"include":[explicit,{"valueSet":["urn:loinc:missing-parts"]}]}}),
    ] {
        let (status, body) = member(&app, resource.clone(), CODES[0]).await;
        assert_eq!(status, 422, "{body}");
        let (status, body) = expand(
            &app,
            resource,
            vec![json!({"name":"count","valueInteger":0})],
        )
        .await;
        assert_eq!(status, 422, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_do_not_enable_unscoped_enumeration_or_part_operations() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for resource in [
        set(json!({"system":SYSTEM,"filter":[filter(PrimaryAxis::Component,"=","LP100-0")]})),
        set(json!({"system":SYSTEM})),
        set(json!({"system":SYSTEM,"concept":[{"code":"LP100-0"}]})),
    ] {
        let (status, body) = expand(
            &app,
            resource,
            vec![json!({"name":"activeOnly","valueBoolean":true})],
        )
        .await;
        assert_eq!(status, 422, "{body}");
    }
    let (status, body) = lookup(&app, "LP100-0", "").await;
    assert_eq!(status, 422, "{body}");
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=http://loinc.org/vs/LP100-0",
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=http://loinc.org/vs",
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_editions_and_instance_lookup_keep_selected_source_availability() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = fixture(&pool, "2.81", false).await;
    fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = lookup(&app, CODES[0], "&property=COMPONENT").await;
    assert_eq!(status, 200, "{body}");
    let (status, body) = lookup(&app, CODES[0], "&version=2.81&property=COMPONENT").await;
    assert_eq!(status, 422, "{body}");
    let (status, body) = request(
        &app,
        "GET",
        &format!(
            "/r4/CodeSystem/{old}/$lookup?code={}&property=COMPONENT",
            CODES[0]
        ),
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    for version in ["2.81", "2.82"] {
        let (status, body) = expand(
            &app,
            set(selected(
                version,
                vec![filter(PrimaryAxis::Component, "=", "LP100-0")],
            )),
            vec![],
        )
        .await;
        assert_eq!(status, if version == "2.82" { 200 } else { 422 }, "{body}");
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_filters_observe_commit_rollback_and_link_deletion_without_ttl() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let resource = set(selected(
        "2.82",
        vec![filter(PrimaryAxis::Component, "=", "LP100-0")],
    ));
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM loinc_primary_links WHERE code_system_id=$1 AND property=$2")
        .bind(id)
        .bind(PrimaryAxis::Component.uri())
        .execute(&mut *tx)
        .await
        .unwrap();
    let (status, body) = expand(&app, resource.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), [CODES[0], CODES[1]]);
    tx.rollback().await.unwrap();
    let (status, body) = expand(&app, resource.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), [CODES[0], CODES[1]]);
    sqlx::query("DELETE FROM loinc_primary_links WHERE code_system_id=$1 AND property=$2")
        .bind(id)
        .bind(PrimaryAxis::Component.uri())
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = expand(&app, resource, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 0);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_duplicate_rows_and_ineligible_links_never_change_counted_pages() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool, "2.82", true).await;
    link(&pool, id, 100, CODES[0], PrimaryAxis::Component, "LP100-0").await;
    for (row, kind, system, part_type) in [
        (101, "Radiology", SYSTEM, "COMPONENT"),
        (102, "Primary", "urn:foreign", "COMPONENT"),
        (103, "Primary", SYSTEM, "METHOD"),
    ] {
        link(&pool, id, row, CODES[2], PrimaryAxis::Component, "LP100-0").await;
        sqlx::query("UPDATE loinc_primary_links SET link_type_name=$3,part_code_system=$4,part_type_name=$5 WHERE code_system_id=$1 AND source_row=$2").bind(id).bind(row).bind(kind).bind(system).bind(part_type).execute(&pool).await.unwrap();
    }
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let resource = set(selected(
        "2.82",
        vec![filter(PrimaryAxis::Component, "in", "LP100-0,LP100-0")],
    ));
    for (offset, expected) in [(0, vec![CODES[0]]), (1, vec![CODES[1]]), (2, vec![])] {
        let (status, body) = expand(
            &app,
            resource.clone(),
            vec![
                json!({"name":"count","valueInteger":1}),
                json!({"name":"offset","valueInteger":offset}),
            ],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], 2);
        assert_eq!(codes(&body), expected);
    }
    let (status, body) = expand(
        &app,
        resource,
        vec![json!({"name":"count","valueInteger":0})],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 2);
    assert!(codes(&body).is_empty());
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_mixed_import_intersection_and_versioned_exclusion_preserve_identity() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.81", true).await;
    fixture(&pool, "2.82", true).await;
    insert_system(&pool,json!({"resourceType":"CodeSystem","url":"urn:finite:parts","content":"complete","concept":[{"code":"F"}]})).await;
    let imported = json!({"resourceType":"ValueSet","url":"urn:loinc:part:intersection","compose":{"include":[{"system":SYSTEM,"version":"2.82","concept":[{"code":CODES[1]},{"code":CODES[2]}]}]}});
    sqlx::query("INSERT INTO value_sets(id,url,status,content) VALUES($1,'urn:loinc:part:intersection','active',$2)").bind(Uuid::new_v4()).bind(sqlx::types::Json(imported)).execute(&pool).await.unwrap();
    let mut selection = selected("2.82", vec![filter(PrimaryAxis::Component, "=", "LP100-0")]);
    selection["valueSet"] = json!(["urn:loinc:part:intersection"]);
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[selection,{"system":"urn:finite:parts"}],"exclude":[{"system":SYSTEM,"version":"2.81","concept":[{"code":CODES[1]}]}]}});
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = expand(&app, resource, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 2);
    let entries = body["expansion"]["contains"].as_array().unwrap();
    assert!(entries
        .iter()
        .any(|e| e["system"] == SYSTEM && e["version"] == "2.82" && e["code"] == CODES[1]));
    assert!(entries
        .iter()
        .any(|e| e["system"] == "urn:finite:parts" && e["code"] == "F"));
    // A finite LOINC edition retains generic package semantics, without a native source flag.
    let finite = insert_system(&pool,json!({"resourceType":"CodeSystem","url":SYSTEM,"version":"2.80","content":"complete","caseSensitive":false,"property":[{"code":"COMPONENT","type":"code"}],"concept":[{"code":"LP100-0","display":"Finite Part","property":[{"code":"COMPONENT","valueCode":"LP123-4"}]}]})).await;
    sqlx::query("UPDATE concepts SET properties='[{\"code\":\"COMPONENT\",\"valueCode\":\"LP123-4\"}]' WHERE code_system_id=$1").bind(finite).execute(&pool).await.unwrap();
    let resource = set(
        json!({"system":SYSTEM,"version":"2.80","filter":[{"property":"COMPONENT","op":"=","value":"LP123-4"}]}),
    );
    let (status, body) = expand(&app, resource, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["LP100-0"]);
    let (status, body) = lookup(&app, "LP100-0", "&version=2.80&property=COMPONENT").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(values(&body, "COMPONENT", "valueCode"), ["LP123-4"]);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_capabilities_are_release_scoped_and_do_not_claim_part_registry() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.81", false).await;
    fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = request(&app, "GET", "/r4/metadata?mode=terminology", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    let system = body["codeSystem"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["uri"] == SYSTEM)
        .unwrap();
    assert_eq!(system["subsumption"], false);
    for (version, loaded) in [("2.81", false), ("2.82", true)] {
        let v = system["version"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["code"] == version)
            .unwrap();
        for axis in PrimaryAxis::all() {
            let entry = v["filter"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["code"] == axis.code());
            assert_eq!(entry.is_some(), loaded, "{v}");
            if let Some(entry) = entry {
                assert_eq!(entry["op"], json!(["=", "exists", "in"]));
            }
        }
        assert!(v["extension"]
            .to_string()
            .contains("not an independently loaded Part registry"));
        assert_eq!(
            v["extension"].to_string().contains("Primary.csv: loaded"),
            loaded
        );
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_parts_malformed_operators_values_and_lists_fail_without_partial_results() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82", true).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let too_many = vec!["LP100-0"; 257].join(",");
    for (op, value) in [
        ("=", "Generated label"),
        ("=", "LP100-00"),
        ("in", "LP100-0,"),
        ("in", too_many.as_str()),
        ("exists", "yes"),
        ("regex", "LP.*"),
        ("is-a", "LP100-0"),
    ] {
        let (status, body) = expand(
            &app,
            set(selected(
                "2.82",
                vec![filter(PrimaryAxis::Component, op, value)],
            )),
            vec![],
        )
        .await;
        assert_eq!(status, 422, "{op} {value}: {body}");
    }
    cleanup(admin, pool, schema).await;
}
