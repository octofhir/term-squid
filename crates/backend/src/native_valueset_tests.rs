//! Generated RF2 projection fixtures exercise provider semantics, not clinical coverage.
use super::*;
use crate::store::{ValueSetOperation, ValueSetPage, ValueSetRequest, ValueSetResult};
use term_squid_core::snomed::{INTERNATIONAL, SYSTEM};
async fn native_query(
    store: &PostgresStore,
    request: ValueSetRequest<'_>,
    operation: ValueSetOperation<'_>,
) -> Result<ValueSetPage, error::AppError> {
    match store.query_value_set(request, operation).await? {
        ValueSetResult::Native(page) => Ok(page),
        ValueSetResult::Finite(_) => panic!("Native fixtures must use the projection provider"),
    }
}
const VERSION: &str = "http://snomed.info/sct/900000000000207008/version/20260201";
async fn fixture(pool: &PgPool) -> Uuid {
    let id = native_test_system(pool, VERSION).await;
    sqlx::query("INSERT INTO snomed_releases VALUES($1,$2,'2026-02-01','900000000000509007','en',repeat('a',64),'{}')")
        .bind(id).bind(INTERNATIONAL).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Fixture '||n,CASE WHEN n=6 THEN '[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb ELSE '[]'::jsonb END FROM generate_series(1,6)n UNION ALL SELECT $1,'410662002','Concept model attribute','[]'::jsonb UNION ALL SELECT $1,'111','Fixture attribute','[]'::jsonb")
        .bind(id).execute(pool).await.unwrap();
    for (edge, source, destination, kind, active) in [
        ("1", "2", "1", "116680003", true),
        ("2", "3", "2", "116680003", true),
        ("3", "4", "1", "116680003", false),
        ("4", "4", "1", "999", true),
        ("5", "3", "5", "111", true),
        ("6", "111", "410662002", "116680003", true),
        ("7", "6", "1", "116680003", false),
    ] {
        sqlx::query("INSERT INTO snomed_relationships VALUES($1,$2,'2026-02-01',$6,'1',$3,$4,0,$5,'900000000000011006','900000000000451002')")
            .bind(id).bind(edge).bind(source).bind(destination).bind(kind).bind(active).execute(pool).await.unwrap();
    }
    id
}
fn inline(include: Value, exclude: Value) -> Value {
    json!({"resourceType":"ValueSet","compose":{"include":include,"exclude":exclude}})
}
fn selection(op: &str, value: &str) -> Value {
    json!({"system":SYSTEM,"version":VERSION,"filter":[{"property":"concept","op":op,"value":value}]})
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
fn codes(body: &Value) -> Vec<&str> {
    body["expansion"]["contains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["code"].as_str().unwrap())
        .collect()
}
async fn member(app: &axum::Router, resource: &Value, code: &str) -> (u16, Value) {
    request(
        app,
        "POST",
        "/r4/ValueSet/$validate-code",
        inline_parameters(
            resource.clone(),
            vec![
                json!({"name":"system","valueUri":SYSTEM}),
                json!({"name":"code","valueCode":code}),
            ],
        ),
    )
    .await
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
async fn native_sets_filters_pages_membership_imports_and_errors() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (op, anchor, expected) in [
        ("is-a", "1", vec!["1", "2", "3"]),
        ("descendent-of", "1", vec!["2", "3"]),
        ("generalizes", "3", vec!["1", "2", "3"]),
    ] {
        let resource = inline(json!([selection(op, anchor)]), json!([]));
        let (status, body) = expand(&app, resource.clone(), vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), expected);
        assert_eq!(body["expansion"]["total"], expected.len());
        for code in ["1", "2", "3", "4", "6", "missing"] {
            let (status, body) = member(&app, &resource, code).await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(
                parameter(&body, "result")["valueBoolean"],
                expected.contains(&code)
            );
        }
    }
    let resource = inline(
        json!([selection("is-a", "1")]),
        json!([{"system":SYSTEM,"version":VERSION,"concept":[{"code":"2"}]}]),
    );
    let (status, body) = expand(
        &app,
        resource.clone(),
        vec![
            json!({"name":"offset","valueInteger":1}),
            json!({"name":"count","valueInteger":1}),
        ],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["3"]);
    assert_eq!(body["expansion"]["total"], 2);
    let (_, body) = member(&app, &resource, "2").await;
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    for (offset, count) in [(50, 10), (0, 0), (i64::MAX, 10)] {
        let (status, body) = expand(
            &app,
            resource.clone(),
            vec![
                json!({"name":"offset","valueInteger":offset}),
                json!({"name":"count","valueInteger":count}),
            ],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert!(codes(&body).is_empty());
        assert_eq!(body["expansion"]["total"], 2);
    }
    let mut and = selection("is-a", "1");
    and["filter"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"111","op":"=","value":"5"}));
    let (status, body) = expand(&app, inline(json!([and]), json!([])), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["3"]);
    let (status, body) = expand(
        &app,
        inline(json!([selection("is-not-a", "1")]), json!([])),
        vec![],
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["111", "4", "410662002", "5", "6"]);
    let mut active = inline(json!([{"system":SYSTEM,"version":VERSION}]), json!([]));
    active["compose"]["inactive"] = false.into();
    let (status, body) = expand(&app, active.clone(), vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 7);
    assert!(!codes(&body).contains(&"6"));
    let (_, body) = member(&app, &active, "6").await;
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    let (status,body) = expand(&app,inline(json!([{"system":SYSTEM,"version":VERSION,"concept":[{"code":"3","display":"Custom"}]}]),json!([])),vec![json!({"name":"filter","valueString":"custom"})]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["3"]);
    assert_eq!(body["expansion"]["contains"][0]["display"], "Custom");
    // Duplicate explicit concepts retain the finite engine's last-display policy.
    let duplicate = inline(
        json!([{"system":SYSTEM,"version":VERSION,"concept":[{"code":"3","display":"Old override"},{"code":"3"}]}]),
        json!([]),
    );
    let (status, body) = expand(&app, duplicate, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["contains"][0]["display"], "Fixture 3");
    insert_set(&pool, Some("a"), json!({"include":[selection("is-a","1")]})).await;
    insert_set(&pool,Some("b"),json!({"include":[{"system":SYSTEM,"version":VERSION,"concept":[{"code":"2"},{"code":"4"}]}]})).await;
    let root = insert_set(
        &pool,
        Some("root"),
        json!({"include":[{"valueSet":["urn:set|a","urn:set|b"]}]}),
    )
    .await;
    for path in [
        format!("/r4/ValueSet/{root}/$expand"),
        "/r4/ValueSet/$expand?url=urn:set%7Croot".into(),
    ] {
        let (status, body) = request(&app, "GET", &path, Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(codes(&body), ["2"]);
    }
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{root}/$validate-code?system={SYSTEM}&code=2"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    let (status,body) = expand(&app,inline(json!([selection("is-a","1"),{"system":SYSTEM,"version":VERSION,"concept":[{"code":"5"}]}]),json!([])),vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(codes(&body), ["1", "2", "3", "5"]);
    for bad in [
        inline(json!([selection("is-a", "999999")]), json!([])),
        inline(json!([selection("in", "1")]), json!([])),
        inline(
            json!([{"system":SYSTEM,"version":VERSION,"filter":[{"property":"constraint","op":"=","value":"<<1"}]}]),
            json!([]),
        ),
        inline(
            json!([{"system":SYSTEM,"version":VERSION,"concept":[{"code":"missing"}]}]),
            json!([]),
        ),
    ] {
        let (status, body) = expand(&app, bad, vec![]).await;
        assert_eq!(status, 422, "{body}");
    }
    insert_set(
        &pool,
        Some("cycle"),
        json!({"include":[{"valueSet":["urn:set|cycle"]},selection("is-a","1")]}),
    )
    .await;
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=urn:set%7Ccycle",
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    let oversized = inline(
        json!([{"system":SYSTEM,"version":VERSION,"concept":(0..4096).map(|_|json!({"code":"1"})).collect::<Vec<_>>()}]),
        json!([]),
    );
    let (status, body) = expand(&app, oversized, vec![]).await;
    assert_eq!(status, 422, "{body}");
    drop(app);
    cleanup(admin, pool, schema).await;
}
#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_implicit_editions_versions_and_unversioned_exclusions() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    fixture(&pool).await;
    let uk_version = "http://snomed.info/sct/83821000000107/version/20260211";
    let uk = native_test_system(&pool, uk_version).await;
    sqlx::query("INSERT INTO snomed_releases VALUES($1,'83821000000107','2026-02-11','900000000000509007','en',repeat('b',64),'{}')").bind(uk).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'1','UK only','[]')").bind(uk).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (url, total) in [
        (format!("{SYSTEM}?fhir_vs"), 8),
        (format!("{SYSTEM}/{INTERNATIONAL}?fhir_vs=isa/1"), 3),
        (format!("{VERSION}?fhir_vs=isa/1"), 3),
        (format!("{uk_version}?fhir_vs"), 1),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            json!({"resourceType":"Parameters","parameter":[{"name":"url","valueUri":url}]}),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], total);
        let expected = if total == 1 { uk_version } else { VERSION };
        assert_eq!(body["expansion"]["contains"][0]["version"], expected);
    }
    for (version, total) in [(VERSION, 8), (uk_version, 1)] {
        let resource = inline(json!([{"system":SYSTEM,"version":version}]), json!([]));
        let (status, body) = expand(&app, resource, vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], total);
    }
    for suffix in ["/%", "/_"] {
        let bogus = format!("{SYSTEM}{suffix}");
        let resource = inline(json!([{"system":SYSTEM,"version":bogus}]), json!([]));
        let (status, body) = expand(&app, resource, vec![]).await;
        assert_eq!(status,404,"An unavailable literal version must never select a release through LIKE wildcards: {body}");
    }
    let resource = inline(
        json!([{"system":SYSTEM,"version":VERSION}]),
        json!([{"system":SYSTEM}]),
    );
    let (status, body) = expand(&app, resource, vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"],0,"Unversioned whole-system exclusion removes every version, even codes absent in the latest release");
    let (status,body) = request(&app,"POST","/r4/ValueSet/$validate-code",json!({"resourceType":"Parameters","parameter":[{"name":"url","valueUri":format!("{VERSION}?fhir_vs=isa/1")},{"name":"system","valueUri":SYSTEM},{"name":"systemVersion","valueString":uk_version},{"name":"code","valueCode":"2"}]})).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    drop(app);
    cleanup(admin, pool, schema).await;
}
#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_whole_system_exceeds_finite_limit_without_enumerating_a_response() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = native_test_system(&pool, VERSION).await;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Generated '||n,'[]'::jsonb FROM generate_series(1,100005)n").bind(id).execute(&pool).await.unwrap();
    let store = PostgresStore::new(pool.clone());
    let resource = inline(json!([{"system":SYSTEM,"version":VERSION}]), json!([]));
    let req = ValueSetRequest {
        url: "urn:inline",
        version: None,
        id: None,
        inline: Some(&resource),
        system_context: None,
    };
    let result = native_query(
        &store,
        req,
        ValueSetOperation::Expand {
            filter: None,
            offset: 100000,
            count: 2,
        },
    )
    .await
    .unwrap();
    assert_eq!(result.total, 100005);
    assert_eq!(result.members.len(), 2);
    let result = native_query(
        &store,
        req,
        ValueSetOperation::Find {
            system: SYSTEM,
            version: Some(VERSION),
            code: "100005",
        },
    )
    .await
    .unwrap();
    assert_eq!(result.members.len(), 1);
    assert_eq!(result.members[0].code, "100005");
    let result = native_query(
        &store,
        req,
        ValueSetOperation::Find {
            system: SYSTEM,
            version: Some(VERSION),
            code: "missing",
        },
    )
    .await
    .unwrap();
    assert!(result.members.is_empty());
    sqlx::query("UPDATE concepts SET display=repeat('x',64000) WHERE code_system_id=$1 AND code IN(SELECT n::text FROM generate_series(1,100)n)").bind(id).execute(&pool).await.unwrap();
    let err = native_query(
        &store,
        req,
        ValueSetOperation::Expand {
            filter: Some("xxxx"),
            offset: 0,
            count: 100,
        },
    )
    .await
    .expect_err("Oversized pages must fail before JSON crosses the process boundary");
    assert!(err.to_string().contains("32 MiB"), "{err}");
    let smaller = native_query(
        &store,
        req,
        ValueSetOperation::Expand {
            filter: Some("xxxx"),
            offset: 0,
            count: 1,
        },
    )
    .await
    .unwrap();
    assert_eq!(smaller.total, 100);
    assert_eq!(smaller.members.len(), 1);
    drop(store);
    cleanup(admin, pool, schema).await;
}

async fn wait_for_projection_reader(admin: &PgPool, schema: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let blocked:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation=to_regclass($1) AND NOT granted)")
                .bind(format!("{schema}.concepts")).fetch_one(admin).await.unwrap();
            if blocked { break; }
            tokio::task::yield_now().await;
        }
    }).await.expect("Reader must reach the locked projection inside its snapshot");
}
#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_sets_snapshot_commits_rollback_and_fresh_negative_results() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let resource = inline(json!([selection("is-a", "1")]), json!([]));
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("LOCK concepts IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let reader = {
        let store = store.clone();
        let resource = resource.clone();
        tokio::spawn(async move {
            native_query(
                &store,
                ValueSetRequest {
                    url: "urn:inline",
                    version: None,
                    id: None,
                    inline: Some(&resource),
                    system_context: None,
                },
                ValueSetOperation::Expand {
                    filter: None,
                    offset: 0,
                    count: 10,
                },
            )
            .await
            .unwrap()
        })
    };
    wait_for_projection_reader(&admin, &schema).await;
    // The blocked reader has already selected the old release in its RR snapshot.
    sqlx::query(
        "UPDATE snomed_relationships SET destination_code='5' WHERE code_system_id=$1 AND id='2'",
    )
    .bind(id)
    .execute(&mut *writer)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE concepts SET display='New generation' WHERE code_system_id=$1 AND code='1'",
    )
    .bind(id)
    .execute(&mut *writer)
    .await
    .unwrap();
    writer.commit().await.unwrap();
    let old = reader.await.unwrap();
    assert_eq!(old.total, 3);
    assert_eq!(old.members[0].display.as_deref(), Some("Fixture 1"));
    let req = ValueSetRequest {
        url: "urn:inline",
        version: None,
        id: None,
        inline: Some(&resource),
        system_context: None,
    };
    let new = native_query(
        &store,
        req,
        ValueSetOperation::Expand {
            filter: None,
            offset: 0,
            count: 10,
        },
    )
    .await
    .unwrap();
    assert_eq!(new.total, 2);
    assert_eq!(new.members[0].display.as_deref(), Some("New generation"));
    let find = ValueSetOperation::Find {
        system: SYSTEM,
        version: Some(VERSION),
        code: "3",
    };
    assert!(native_query(&store, req, find)
        .await
        .unwrap()
        .members
        .is_empty());
    let mut writer = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE snomed_relationships SET destination_code='2' WHERE code_system_id=$1 AND id='2'",
    )
    .bind(id)
    .execute(&mut *writer)
    .await
    .unwrap();
    writer.rollback().await.unwrap();
    assert!(native_query(&store, req, find)
        .await
        .unwrap()
        .members
        .is_empty());
    sqlx::query(
        "UPDATE snomed_relationships SET destination_code='2' WHERE code_system_id=$1 AND id='2'",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        native_query(&store, req, find).await.unwrap().members[0].code,
        "3",
        "A committed edge must invalidate the earlier negative result"
    );
    let set = insert_set(
        &pool,
        Some("changing"),
        json!({"include":[selection("is-a","1")]}),
    )
    .await;
    let stored = ValueSetRequest {
        url: "urn:set",
        version: Some("changing"),
        id: None,
        inline: None,
        system_context: None,
    };
    assert_eq!(
        native_query(&store, stored, find)
            .await
            .unwrap()
            .members
            .len(),
        1
    );
    sqlx::query(
        "UPDATE value_sets SET content=jsonb_set(content,'{compose,exclude}',$2) WHERE id=$1",
    )
    .bind(set)
    .bind(sqlx::types::Json(
        json!([{"system":SYSTEM,"version":VERSION,"concept":[{"code":"3"}]}]),
    ))
    .execute(&pool)
    .await
    .unwrap();
    assert!(native_query(&store, stored, find)
        .await
        .unwrap()
        .members
        .is_empty());
    drop(store);
    cleanup(admin, pool, schema).await;
}
#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_sets_admission_cancellation_and_ancestor_limits() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let url = format!("{VERSION}?fhir_vs=isa/1");
    let req = ValueSetRequest {
        url: &url,
        version: None,
        id: None,
        inline: None,
        system_context: None,
    };
    let operation = ValueSetOperation::Find {
        system: SYSTEM,
        version: Some(VERSION),
        code: "3",
    };
    let held = store.pause_value_set_requests().await;
    assert!(matches!(
        native_query(&store, req, operation).await,
        Err(error::AppError::Overloaded)
    ));
    drop(held);
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("LOCK concepts IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let reader = {
        let store = store.clone();
        let url = url.clone();
        tokio::spawn(async move {
            native_query(
                &store,
                ValueSetRequest {
                    url: &url,
                    version: None,
                    id: None,
                    inline: None,
                    system_context: None,
                },
                ValueSetOperation::Find {
                    system: SYSTEM,
                    version: Some(VERSION),
                    code: "3",
                },
            )
            .await
        })
    };
    wait_for_projection_reader(&admin, &schema).await;
    assert_eq!(store.native_available_permits(), 3);
    reader.abort();
    assert!(reader.await.unwrap_err().is_cancelled());
    assert_eq!(
        store.native_available_permits(),
        4,
        "Cancellation must return the work permit"
    );
    writer.rollback().await.unwrap();
    assert_eq!(
        native_query(&store, req, operation)
            .await
            .unwrap()
            .members
            .len(),
        1
    );
    // Finding the requested ancestor early does not justify a truncated successful answer.
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Ancestor '||n,'[]'::jsonb FROM generate_series(10000,18200)n").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships SELECT $1,'chain-'||n,'2026-02-01'::date,true,'1',n::text,(n+1)::text,0,'116680003','900000000000011006','900000000000451002' FROM generate_series(10000,18199)n").bind(id).execute(&pool).await.unwrap();
    sqlx::query("ANALYZE snomed_relationships")
        .execute(&pool)
        .await
        .unwrap();
    let resource = inline(json!([selection("is-a", "10001")]), json!([]));
    let err = native_query(
        &store,
        ValueSetRequest {
            url: "urn:inline",
            version: None,
            id: None,
            inline: Some(&resource),
            system_context: None,
        },
        ValueSetOperation::Find {
            system: SYSTEM,
            version: Some(VERSION),
            code: "10000",
        },
    )
    .await
    .expect_err("Traversal must reject a partial positive result");
    assert!(err.to_string().contains("traversal limit"), "{err}");
    drop(store);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_small_and_broad_selection_branches_have_identical_set_semantics() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = fixture(&pool).await;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Branch '||n,'[]'::jsonb FROM generate_series(10000,10255)n").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships SELECT $1,'branch-'||n,'2026-02-01'::date,true,'1',n::text,'10000',0,'116680003','900000000000011006','900000000000451002' FROM generate_series(10001,10255)n").bind(id).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let resource = inline(json!([selection("is-a", "10000")]), json!([]));
    for total in [256, 257] {
        let (status, body) = expand(
            &app,
            resource.clone(),
            vec![json!({"name":"count","valueInteger":1000})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], total);
        let expected: Vec<_> = (10000..10000 + total).map(|n| n.to_string()).collect();
        assert_eq!(codes(&body), expected);
        let mut excluded = resource.clone();
        excluded["compose"]["exclude"] =
            json!([{"system":SYSTEM,"version":VERSION,"concept":[{"code":"10002"}]}]);
        let (status, body) = expand(
            &app,
            excluded,
            vec![
                json!({"name":"offset","valueInteger":1}),
                json!({"name":"count","valueInteger":2}),
            ],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["expansion"]["total"], total - 1);
        assert_eq!(codes(&body), ["10001", "10003"]);
        if total == 256 {
            sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'10256','Branch 10256','[]')").bind(id).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO snomed_relationships VALUES($1,'branch-10256','2026-02-01',true,'1','10256','10000',0,'116680003','900000000000011006','900000000000451002')").bind(id).execute(&pool).await.unwrap();
        }
    }
    for (code, expected) in [("10256", true), ("1", false), ("missing", false)] {
        let (status, body) = member(&app, &resource, code).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], expected);
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn finite_dispatch_borrows_shared_plans_and_preserves_old_generations() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = insert_system(&pool,json!({"resourceType":"CodeSystem","url":"urn:finite","version":"1","content":"complete","concept":[{"code":"a","display":"Original"}]})).await;
    insert_set(
        &pool,
        Some("stable"),
        json!({"include":[{"system":"urn:finite","version":"1"}]}),
    )
    .await;
    let store = PostgresStore::new(pool.clone());
    let prepared = store
        .expand_compose("urn:set", Some("stable"))
        .await
        .unwrap();
    let req = ValueSetRequest {
        url: "urn:set",
        version: Some("stable"),
        id: None,
        inline: None,
        system_context: None,
    };
    let operation = ValueSetOperation::Find {
        system: "urn:finite",
        version: Some("1"),
        code: "a",
    };
    let ValueSetResult::Finite(old) = store.query_value_set(req, operation).await.unwrap() else {
        panic!("Finite data must retain its prepared plan")
    };
    assert!(
        Arc::ptr_eq(&prepared.members, &old.members),
        "Dispatch must share the immutable plan, not clone member payloads"
    );
    sqlx::query("UPDATE code_systems SET content=jsonb_set(content,'{concept,0,display}','\"Updated\"') WHERE id=$1").bind(id).execute(&pool).await.unwrap();
    let ValueSetResult::Finite(new) = store.query_value_set(req, operation).await.unwrap() else {
        panic!("Expected finite plan")
    };
    assert!(!Arc::ptr_eq(&old.members, &new.members));
    assert_eq!(
        old.members
            .find("urn:finite", Some("1"), "a")
            .unwrap()
            .display
            .as_deref(),
        Some("Original")
    );
    assert_eq!(
        new.members
            .find("urn:finite", Some("1"), "a")
            .unwrap()
            .display
            .as_deref(),
        Some("Updated")
    );
    drop(store);
    cleanup(admin, pool, schema).await;
}
