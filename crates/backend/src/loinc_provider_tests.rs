//! Generated primary-table projections test scope and freshness, not clinical coverage.
use super::*;

const LOINC: &str = "http://loinc.org";
const LOINC_VS: &str = "http://loinc.org/vs";
const CODES: [&str; 4] = ["1000-1", "1001-9", "1002-7", "1003-5"];

fn metadata(version: &str) -> Value {
    json!({"resourceType":"CodeSystem","url":LOINC,"version":version,"content":"not-present","caseSensitive":false})
}

fn properties(status: &str, class: &str, class_type: &str, method: Option<&str>) -> Value {
    let mut values = vec![
        json!({"code":"source-table","valueString":"Loinc.csv"}),
        json!({"code":"raw-COMPONENT","valueString":"Generated component"}),
        json!({"code":"STATUS","valueString":status}),
        json!({"code":"CLASS","valueString":class}),
        json!({"code":"CLASSTYPE","valueString":class_type}),
        json!({"code":"ORDER_OBS","valueString":"Both"}),
        json!({"code":"inactive","valueBoolean":status == "DEPRECATED"}),
    ];
    if let Some(method) = method {
        values.push(json!({"code":"raw-METHOD_TYP","valueString":method}));
    }
    Value::Array(values)
}

async fn fixture(pool: &PgPool, version: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider,updated_at) VALUES($1,$2,$3,'active','Generated LOINC fixture',$4,'loinc-csv',CASE WHEN $3='2.81' THEN '2026-01-01'::timestamptz ELSE '2026-02-01'::timestamptz END)")
        .bind(id).bind(LOINC).bind(version).bind(sqlx::types::Json(metadata(version))).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases(code_system_id,version,fingerprint,provenance) VALUES($1,$2,repeat('a',64),'{\"generatedFixture\":true,\"scope\":\"Loinc.csv\"}')")
        .bind(id).bind(version).execute(pool).await.unwrap();
    for (index, (status, class, class_type, method)) in [
        ("ACTIVE", "CHEM", "1", None),
        ("TRIAL", "CHEM", "1", Some("Enzymatic")),
        ("DISCOURAGED", "CHEM", "2", None),
        ("DEPRECATED", "HEM", "1", Some("Calculation")),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query(
            "INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(CODES[index])
        .bind(format!("Generated {version} {index}"))
        .bind(sqlx::types::Json(properties(
            status, class, class_type, method,
        )))
        .execute(pool)
        .await
        .unwrap();
    }
    id
}

fn inline(include: Value, exclude: Value) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":include,"exclude":exclude}})
}

fn scoped(version: &str, filters: Vec<Value>) -> Value {
    let mut all = vec![json!({"property":"source-table","op":"=","value":"Loinc.csv"})];
    all.extend(filters);
    json!({"system":LOINC,"version":version,"filter":all})
}

fn explicit(version: &str, codes: &[&str]) -> Value {
    json!({"system":LOINC,"version":version,"concept":codes.iter().map(|code|json!({"code":code})).collect::<Vec<_>>()})
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

async fn member(
    app: &axum::Router,
    resource: Value,
    system: &str,
    code: &str,
    extra: Vec<Value>,
) -> (u16, Value) {
    let mut parameters = vec![
        json!({"name":"system","valueUri":system}),
        json!({"name":"code","valueCode":code}),
    ];
    parameters.extend(extra);
    request(
        app,
        "POST",
        "/r4/ValueSet/$validate-code",
        inline_parameters(resource, parameters),
    )
    .await
}

async fn implicit(app: &axum::Router, code: &str, extra: Vec<Value>) -> (u16, Value) {
    let mut parameters = vec![
        json!({"name":"url","valueUri":LOINC_VS}),
        json!({"name":"system","valueUri":LOINC}),
        json!({"name":"code","valueCode":code}),
    ];
    parameters.extend(extra);
    request(
        app,
        "POST",
        "/r4/ValueSet/$validate-code",
        json!({"resourceType":"Parameters","parameter":parameters}),
    )
    .await
}

fn codes(body: &Value) -> Vec<&str> {
    body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["code"].as_str().unwrap())
        .collect()
}

fn assert_result(body: &Value, expected: bool) {
    assert_eq!(
        parameter(body, "result")["valueBoolean"],
        expected,
        "{body}"
    );
}

fn assert_context(body: &Value, version: &str, display: &str) {
    assert_result(body, true);
    assert_eq!(parameter(body, "version")["valueString"], version, "{body}");
    assert_eq!(parameter(body, "display")["valueString"], display, "{body}");
}

async fn cleanup(admin: PgPool, pool: PgPool, schema: String) {
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_primary_reads_preserve_numeric_and_unloaded_domain_semantics() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = fixture(&pool, "2.81").await;
    fixture(&pool, "2.82").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (index, code) in CODES.into_iter().enumerate() {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/$lookup?system={LOINC}&code={code}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "version")["valueString"], "2.82");
        assert_eq!(
            parameter(&body, "display")["valueString"],
            format!("Generated 2.82 {index}")
        );
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/$validate-code?url={LOINC}&code={code}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_context(&body, "2.82", &format!("Generated 2.82 {index}"));
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$validate-code",
            json!({"resourceType":"Parameters","parameter":[
                {"name":"system","valueUri":LOINC},
                {"name":"code","valueCode":code}
            ]}),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_context(&body, "2.82", &format!("Generated 2.82 {index}"));
        for operation in ["$lookup", "$validate-code"] {
            let mut parameters = vec![
                json!({"name":"coding","valueCoding":{"system":LOINC,"version":"2.81","code":code}}),
            ];
            if operation == "$validate-code" {
                parameters.push(json!({"name":"url","valueUri":LOINC}));
            }
            let (status, body) = request(
                &app,
                "POST",
                &format!("/r4/CodeSystem/{operation}"),
                json!({"resourceType":"Parameters","parameter":parameters}),
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "version")["valueString"], "2.81");
            assert_eq!(
                parameter(&body, "display")["valueString"],
                format!("Generated 2.81 {index}")
            );
            if operation == "$validate-code" {
                assert_result(&body, true);
            }
        }
        let (status, body) = implicit(&app, code, vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_context(&body, "2.82", &format!("Generated 2.82 {index}"));
    }
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/$lookup?system={LOINC}&code=9999-9"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 404, "{body}");
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/$validate-code?url={LOINC}&code=9999-9"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    let (status, body) = implicit(&app, "9999-9", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    for code in ["LA1-9", "LP1-2", "LL1-0", "not-a-code"] {
        for operation in ["$lookup", "$validate-code"] {
            let (status, body) = request(
                &app,
                "GET",
                &format!("/r4/CodeSystem/{operation}?system={LOINC}&url={LOINC}&code={code}"),
                Value::Null,
            )
            .await;
            assert_eq!(
                status, 422,
                "Unloaded code families cannot be negative results: {body}"
            );
        }
        let (status, body) = implicit(&app, code, vec![]).await;
        assert_eq!(status, 422, "{body}");
    }
    for field in ["systemVersion", "valueSetVersion"] {
        let (status, body) = implicit(
            &app,
            CODES[0],
            vec![json!({"name":field,"valueString":"2.81"})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_context(&body, "2.81", "Generated 2.81 0");
    }
    let (status, body) = implicit(
        &app,
        CODES[0],
        vec![json!({"name":"systemVersion","valueString":"missing"})],
    )
    .await;
    assert_eq!(
        status, 404,
        "An unavailable release is not a missing code: {body}"
    );
    let (status, body) = implicit(
        &app,
        CODES[0],
        vec![json!({"name":"display","valueString":"Wrong display"})],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{old}/$lookup?code={}", CODES[0]),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "version")["valueString"], "2.81");
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{old}/$validate-code?code=LA1-9"),
        Value::Null,
    )
    .await;
    assert_eq!(
        status, 422,
        "Instance addressing must retain provider scope: {body}"
    );
    sqlx::query("UPDATE concepts SET display=NULL WHERE code_system_id=$1 AND code=$2")
        .bind(old)
        .bind(CODES[0])
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{old}/$lookup?code={}", CODES[0]),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        parameter(&body, "display")["valueString"],
        CODES[0],
        "A term without a supplied display must not emit an empty FHIR primitive: {body}"
    );
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_source_scoped_filters_inactive_and_pages_are_exact() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.82").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let all = inline(json!([scoped("2.82", vec![])]), json!([]));
    let (status, body) = expand(
        &app,
        all.clone(),
        vec![
            json!({"name":"offset","valueInteger":1}),
            json!({"name":"count","valueInteger":2}),
        ],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 4);
    assert_eq!(codes(&body), [CODES[1], CODES[2]]);
    for entry in body["expansion"]["contains"].as_array().unwrap() {
        assert_eq!(entry["system"], LOINC);
        assert_eq!(entry["version"], "2.82");
    }
    let (status, again) = expand(
        &app,
        all.clone(),
        vec![
            json!({"name":"offset","valueInteger":1}),
            json!({"name":"count","valueInteger":2}),
        ],
    )
    .await;
    assert_eq!(status, 200, "{again}");
    assert_eq!(
        body["expansion"]["contains"],
        again["expansion"]["contains"]
    );
    for (property, op, value, expected) in [
        ("STATUS", "=", "ACTIVE", vec![CODES[0]]),
        ("STATUS", "=", "TRIAL", vec![CODES[1]]),
        ("STATUS", "=", "DISCOURAGED", vec![CODES[2]]),
        ("STATUS", "=", "DEPRECATED", vec![CODES[3]]),
        ("STATUS", "=", "active", vec![]),
        ("CLASSTYPE", "=", "1", vec![CODES[0], CODES[1], CODES[3]]),
        ("CLASSTYPE", "=", "01", vec![]),
        ("raw-METHOD_TYP", "exists", "true", vec![CODES[1], CODES[3]]),
        (
            "raw-METHOD_TYP",
            "exists",
            "false",
            vec![CODES[0], CODES[2]],
        ),
        ("STATUS", "exists", "false", vec![]),
        ("raw-COMPONENT", "=", "Generated component", CODES.to_vec()),
    ] {
        let resource = inline(
            json!([scoped(
                "2.82",
                vec![json!({"property":property,"op":op,"value":value})]
            )]),
            json!([]),
        );
        let (status, body) = expand(&app, resource.clone(), vec![]).await;
        assert_eq!(status, 200, "{property} {op} {value}: {body}");
        assert_eq!(codes(&body), expected, "{property} {op} {value}: {body}");
        for code in CODES.into_iter().chain(["9999-9"]) {
            let (status, body) = member(&app, resource.clone(), LOINC, code, vec![]).await;
            assert_eq!(status, 200, "{body}");
            assert_result(&body, expected.contains(&code));
        }
    }
    let mut active = all.clone();
    active["compose"]["inactive"] = false.into();
    let (status, body) = expand(&app, active.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), CODES[..3]);
    let (status, body) = member(&app, active, LOINC, CODES[3], vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    for (offset, count) in [(100, 2), (0, 0), (i64::MAX, 2)] {
        let (status, body) = expand(
            &app,
            all.clone(),
            vec![
                json!({"name":"offset","valueInteger":offset}),
                json!({"name":"count","valueInteger":count}),
            ],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert!(codes(&body).is_empty());
        assert_eq!(body["expansion"]["total"], 4);
    }
    for resource in [
        inline(json!([{"system":LOINC}]), json!([])),
        inline(
            json!([{"system":LOINC,"filter":[{"property":"STATUS","op":"=","value":"ACTIVE"}]}]),
            json!([]),
        ),
        inline(
            json!([scoped(
                "2.82",
                vec![json!({"property":"COMPONENT","op":"=","value":"LP1-2"})]
            )]),
            json!([]),
        ),
        inline(
            json!([scoped(
                "2.82",
                vec![json!({"property":"raw-COMPONENT","op":"regex","value":".*"})]
            )]),
            json!([]),
        ),
        inline(
            json!([scoped(
                "2.82",
                vec![json!({"property":"ancestor","op":"=","value":"LP1-2"})]
            )]),
            json!([]),
        ),
        inline(json!([explicit("2.82", &["LA1-9"])]), json!([])),
    ] {
        let (status, body) = expand(&app, resource, vec![]).await;
        assert_eq!(
            status, 422,
            "Unsupported or incomplete scopes cannot expand partially: {body}"
        );
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=http://loinc.org/vs&count=0",
        Value::Null,
    )
    .await;
    assert_eq!(
        status, 422,
        "count=0 cannot bypass source completeness: {body}"
    );
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_mixed_imports_intersect_and_exclusions_keep_version_scope() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = fixture(&pool, "2.81").await;
    fixture(&pool, "2.82").await;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'1004-3','Old only',$2)")
        .bind(old).bind(sqlx::types::Json(properties("ACTIVE","CHEM","1",None))).execute(&pool).await.unwrap();
    let snomed = term_squid_core::snomed::SYSTEM;
    let snomed_version = "http://snomed.info/sct/900000000000207008/version/20260201";
    let snomed_id = native_test_system(&pool, snomed_version).await;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'7','Generated SNOMED','[]')")
        .bind(snomed_id).execute(&pool).await.unwrap();
    let snomed_selection =
        json!({"system":snomed,"version":snomed_version,"concept":[{"code":"7"}]});
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let resource = inline(
        json!([scoped("2.82", vec![]), snomed_selection.clone()]),
        json!([]),
    );
    let (status, body) = expand(&app, resource.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 5);
    let members = body["expansion"]["contains"].as_array().unwrap();
    assert_eq!(
        members
            .iter()
            .filter(|entry| entry["system"] == LOINC)
            .count(),
        4
    );
    assert_eq!(
        members
            .iter()
            .filter(|entry| entry["system"] == snomed)
            .count(),
        1
    );
    let (status, body) = member(&app, resource, snomed, "7", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, snomed_version, "Generated SNOMED");
    insert_set(
        &pool,
        Some("loinc-a"),
        json!({"include":[explicit("2.82",&CODES[..2]),snomed_selection.clone()]}),
    )
    .await;
    insert_set(
        &pool,
        Some("loinc-b"),
        json!({"include":[explicit("2.82",&CODES[1..3]),snomed_selection.clone()]}),
    )
    .await;
    let root=insert_set(&pool,Some("loinc-root"),json!({"include":[{"valueSet":["urn:set|loinc-a","urn:set|loinc-b"]}],"exclude":[snomed_selection.clone()]})).await;
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{root}/$expand"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), [CODES[1]]);
    let (status, body) = request(
        &app,
        "GET",
        &format!(
            "/r4/ValueSet/{root}/$validate-code?system={LOINC}&code={}",
            CODES[1]
        ),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, "2.82", "Generated 2.82 1");
    for (exclude, expected) in [
        (json!({"system":LOINC}), vec![(snomed, "7")]),
        (
            json!({"system":LOINC,"version":"2.81"}),
            vec![(LOINC, CODES[0]), (snomed, "7")],
        ),
    ] {
        let resource = inline(
            json!([
                explicit("2.81", &["1004-3"]),
                explicit("2.82", &[CODES[0]]),
                snomed_selection.clone()
            ]),
            json!([exclude]),
        );
        let (status, body) = expand(&app, resource, vec![]).await;
        assert_eq!(status, 200, "{body}");
        let actual: Vec<_> = body["expansion"]["contains"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                (
                    entry["system"].as_str().unwrap(),
                    entry["code"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(actual,expected,"Unversioned exclusions remove the URL across versions, including old-only codes: {body}");
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_membership_observes_commit_rollback_replacement_and_deletion() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool, "2.81").await;
    let current = fixture(&pool, "2.82").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = implicit(&app, "1004-3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'1004-3','Rolled back',$2)")
        .bind(current).bind(sqlx::types::Json(properties("ACTIVE","CHEM","1",None))).execute(&mut *writer).await.unwrap();
    let (status, body) = implicit(&app, "1004-3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    writer.rollback().await.unwrap();
    let (status, body) = implicit(&app, "1004-3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'1004-3','Committed',$2)")
        .bind(current).bind(sqlx::types::Json(properties("ACTIVE","CHEM","1",None))).execute(&pool).await.unwrap();
    let (status, body) = implicit(&app, "1004-3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, "2.82", "Committed");
    sqlx::query("DELETE FROM concepts WHERE code_system_id=$1 AND code='1004-3'")
        .bind(current)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = implicit(&app, "1004-3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_result(&body, false);
    sqlx::query("UPDATE concepts SET display='Replacement' WHERE code_system_id=$1 AND code=$2")
        .bind(current)
        .bind(CODES[0])
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = implicit(&app, CODES[0], vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, "2.82", "Replacement");
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(current)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = implicit(&app, CODES[0], vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, "2.81", "Generated 2.81 0");
    let (status, body) = implicit(
        &app,
        CODES[0],
        vec![json!({"name":"systemVersion","valueString":"2.82"})],
    )
    .await;
    assert_eq!(
        status, 404,
        "Deleted exact releases cannot produce stale positives: {body}"
    );
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn loinc_concurrent_activation_never_combines_generations() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = fixture(&pool, "2.81").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("LOCK concepts IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let reader = {
        let app = app.clone();
        tokio::spawn(async move { implicit(&app, CODES[0], vec![]).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation=to_regclass($1) AND NOT granted)")
                .bind(format!("{schema}.concepts")).fetch_one(&admin).await.unwrap();
            if blocked {break;}
            tokio::task::yield_now().await;
        }
    }).await.expect("Reader must reach the locked primary-term projection");
    sqlx::query(
        "UPDATE concepts SET display='Retired generation' WHERE code_system_id=$1 AND code=$2",
    )
    .bind(old)
    .bind(CODES[0])
    .execute(&mut *writer)
    .await
    .unwrap();
    let next = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider,updated_at) VALUES($1,$2,'2.82','active','Generated next LOINC',$3,'loinc-csv','2026-02-01')")
        .bind(next).bind(LOINC).bind(sqlx::types::Json(metadata("2.82"))).execute(&mut *writer).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases(code_system_id,version,fingerprint,provenance) VALUES($1,'2.82',repeat('b',64),'{\"generatedFixture\":true,\"scope\":\"Loinc.csv\"}')")
        .bind(next).execute(&mut *writer).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,$2,'Next generation',$3)")
        .bind(next).bind(CODES[0]).bind(sqlx::types::Json(properties("ACTIVE","CHEM","1",None))).execute(&mut *writer).await.unwrap();
    writer.commit().await.unwrap();
    let (status, body) = reader.await.unwrap();
    assert_eq!(status, 200, "{body}");
    assert_result(&body, true);
    let pair = (
        parameter(&body, "version")["valueString"].as_str().unwrap(),
        parameter(&body, "display")["valueString"].as_str().unwrap(),
    );
    assert!(
        [("2.81", "Generated 2.81 0"), ("2.82", "Next generation")].contains(&pair),
        "Only a complete old or complete new snapshot is allowed: {body}"
    );
    let (status, body) = implicit(&app, CODES[0], vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, "2.82", "Next generation");
    drop(app);
    cleanup(admin, pool, schema).await;
}
