//! Opt-in integration checks against a disposable schema in a local PostgreSQL database.
use super::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

async fn request(app: &axum::Router, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/fhir+json")
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status().as_u16();
    (
        status,
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap(),
    )
}
fn parameter<'a>(body: &'a Value, name: &str) -> &'a Value {
    body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == name)
        .unwrap()
}
async fn insert_system(pool: &PgPool, mut resource: Value) -> Uuid {
    let id = Uuid::new_v4();
    resource["id"] = id.to_string().into();
    sqlx::query("INSERT INTO code_systems (id,url,version,status,name,content) VALUES ($1,$2,$3,'active','Test',$4)")
        .bind(id).bind(resource["url"].as_str().unwrap()).bind(resource["version"].as_str()).bind(sqlx::types::Json(&resource)).execute(pool).await.unwrap();
    let prepared =
        term_squid_core::PreparedSystem::prepare(serde_json::from_value(resource).unwrap())
            .unwrap();
    for concept in prepared.concepts.values() {
        sqlx::query(
            "INSERT INTO concepts (code_system_id,code,display,properties) VALUES ($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(&concept.code)
        .bind(&concept.display)
        .bind(sqlx::types::Json(json!([])))
        .execute(pool)
        .await
        .unwrap();
    }
    if prepared.hierarchy_meaning.as_deref() == Some("is-a") {
        for code in prepared.concepts.keys() {
            for (ancestor, depth) in prepared.ancestor_depths(code).unwrap() {
                sqlx::query("INSERT INTO closure_table (code_system_id,ancestor_code,descendant_code,depth) VALUES ($1,$2,$3,$4)")
                    .bind(id).bind(ancestor).bind(code).bind(depth as i32).execute(pool).await.unwrap();
            }
        }
    }
    id
}
async fn insert_set(pool: &PgPool, version: Option<&str>, compose: Value) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO value_sets (id,url,version,status,content) VALUES ($1,'urn:set',$2,'active',$3)")
        .bind(id).bind(version).bind(sqlx::types::Json(json!({"resourceType":"ValueSet","id":id,"url":"urn:set","version":version,"compose":compose}))).execute(pool).await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn subsumption_versions_case_and_atomic_updates() {
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = format!("SET search_path TO {schema}, public");
    let pool = database::pool_options(5)
        .after_connect(move |conn, _| {
            let sql = search_path.clone();
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let v1 = insert_system(&pool, json!({"url":"urn:hierarchy","version":"1","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[{"code":"A","concept":[{"code":"Middle","concept":[{"code":"B"}]}]},{"code":"Other"}]})).await;
    let v2 = insert_system(&pool, json!({"url":"urn:hierarchy","version":"2","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[{"code":"B","concept":[{"code":"A"}]}]})).await;
    let unversioned = insert_system(&pool, json!({"url":"urn:hierarchy","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"A"},{"code":"B"}]})).await;
    let no_hierarchy = insert_system(
        &pool,
        json!({"url":"urn:flat","content":"complete","concept":[{"code":"A"},{"code":"B"}]}),
    )
    .await;
    insert_system(&pool, json!({"url":"urn:sensitive","content":"complete","caseSensitive":true,"hierarchyMeaning":"is-a","concept":[{"code":"A","concept":[{"code":"B"}]}]})).await;
    sqlx::query("UPDATE code_systems SET updated_at=NOW()+interval '1 minute' WHERE id=$1")
        .bind(v2)
        .execute(&pool)
        .await
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (context, a, b, expected) in [
        (
            "$subsumes?system=urn:hierarchy&version=1",
            "a",
            "b",
            "subsumes",
        ),
        (
            "$subsumes?system=urn:hierarchy%7C1",
            "B",
            "a",
            "subsumed-by",
        ),
        (
            "$subsumes?system=urn:hierarchy&version=1",
            "A",
            "a",
            "equivalent",
        ),
        (
            "$subsumes?system=urn:hierarchy&version=1",
            "A",
            "Other",
            "not-subsumed",
        ),
        ("$subsumes?system=urn:hierarchy", "A", "B", "subsumed-by"),
        ("$subsumes?system=urn:sensitive", "A", "B", "subsumes"),
        (&format!("{v1}/$subsumes?"), "a", "B", "subsumes"),
        (
            &format!("{unversioned}/$subsumes?"),
            "A",
            "B",
            "not-subsumed",
        ),
    ] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/{context}&codeA={a}&codeB={b}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "outcome")["valueCode"], expected);
    }
    for path in [
        "/r4/CodeSystem/$subsumes?system=urn:hierarchy&version=1".to_owned(),
        format!("/r4/CodeSystem/{v1}/$subsumes?"),
    ] {
        for (a, b) in [("missing", "B"), ("A", "missing"), ("missing", "missing")] {
            let (status, body) = request(
                &app,
                "GET",
                &format!("{path}&codeA={a}&codeB={b}"),
                Value::Null,
            )
            .await;
            assert_eq!(status, 404);
            assert_eq!(body["resourceType"], "OperationOutcome");
        }
    }
    for (context, expected) in [
        ("$subsumes?system=urn:missing".to_owned(), 404),
        (
            "$subsumes?system=urn:hierarchy&version=missing".to_owned(),
            404,
        ),
        (format!("{}/$subsumes?", Uuid::new_v4()), 404),
        (
            "$subsumes?system=urn:hierarchy%7C1&version=2".to_owned(),
            422,
        ),
        ("$subsumes?system=urn:sensitive".to_owned(), 404),
        (format!("{no_hierarchy}/$subsumes?"), 400),
    ] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/{context}&codeA=a&codeB=b"),
            Value::Null,
        )
        .await;
        assert_eq!(status, expected, "{body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    for (path, expected) in [
        ("/r4/CodeSystem/$subsumes".to_owned(), "subsumed-by"),
        (format!("/r4/CodeSystem/{v1}/$subsumes"), "subsumes"),
        (
            format!("/r4/CodeSystem/{unversioned}/$subsumes"),
            "not-subsumed",
        ),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            &path,
            json!({"resourceType":"Parameters","parameter":[
            {"name":"system","valueUri":"urn:hierarchy"},{"name":"version","valueString":"2"},
            {"name":"codeA","valueCode":"A"},{"name":"codeB","valueCode":"B"}]}),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "outcome")["valueCode"], expected);
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$subsumes?system=urn:hierarchy&version=2&codeA=A&codeB=Other",
        Value::Null,
    )
    .await;
    assert_eq!(status, 404);
    assert_eq!(body["resourceType"], "OperationOutcome");

    let path = format!("/r4/CodeSystem/{v1}/$subsumes?codeA=a&codeB=b");
    // Uncommitted closure writes and rollback cannot change the answer.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM closure_table WHERE code_system_id=$1")
        .bind(v1)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "outcome")["valueCode"], "subsumes");
    tx.rollback().await.unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "outcome")["valueCode"], "subsumes");

    // Both committed states subsume a -> b. Mixing normalized codes from one state
    // with the other state's closure would incorrectly report no relationship.
    let barrier = Arc::new(tokio::sync::Barrier::new(5));
    let writer_pool = pool.clone();
    let writer_barrier = barrier.clone();
    let writer = tokio::spawn(async move {
        writer_barrier.wait().await;
        for generation in 0..32 {
            let (a, b) = if generation % 2 == 0 {
                ("a", "b")
            } else {
                ("A", "B")
            };
            let mut tx = writer_pool.begin().await.unwrap();
            sqlx::query("DELETE FROM closure_table WHERE code_system_id=$1")
                .bind(v1)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("DELETE FROM concepts WHERE code_system_id=$1")
                .bind(v1)
                .execute(&mut *tx)
                .await
                .unwrap();
            tokio::task::yield_now().await;
            let content = json!({"url":"urn:hierarchy","version":"1","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[{"code":a,"concept":[{"code":b}]}]});
            sqlx::query("UPDATE code_systems SET content=$1 WHERE id=$2")
                .bind(sqlx::types::Json(content))
                .bind(v1)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("INSERT INTO concepts (code_system_id,code) VALUES ($1,$2),($1,$3)")
                .bind(v1)
                .bind(a)
                .bind(b)
                .execute(&mut *tx)
                .await
                .unwrap();
            sqlx::query("INSERT INTO closure_table (code_system_id,ancestor_code,descendant_code,depth) VALUES ($1,$2,$3,1)")
                .bind(v1).bind(a).bind(b).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
        }
    });
    let mut readers = Vec::new();
    for _ in 0..4 {
        let app = app.clone();
        let path = path.clone();
        let barrier = barrier.clone();
        readers.push(tokio::spawn(async move {
            barrier.wait().await;
            for _ in 0..64 {
                let (status, body) = request(&app, "GET", &path, Value::Null).await;
                assert_eq!(status, 200, "{body}");
                assert_eq!(parameter(&body, "outcome")["valueCode"], "subsumes");
            }
        }));
    }
    writer.await.unwrap();
    for reader in readers {
        reader.await.unwrap();
    }
    // A committed reversal and subsequent deletion are visible without restarting.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET content=$1 WHERE id=$2")
        .bind(sqlx::types::Json(json!({"url":"urn:hierarchy","version":"1","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[{"code":"B","concept":[{"code":"A"}]}]})))
        .bind(v1).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "UPDATE closure_table SET ancestor_code='B',descendant_code='A' WHERE code_system_id=$1",
    )
    .bind(v1)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "outcome")["valueCode"], "subsumed-by");
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(v1)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 404);
    assert_eq!(body["resourceType"], "OperationOutcome");
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn terminology_http_versions_compose_and_refresh() {
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").expect("TERM_SQUID_TEST_DATABASE_URL");
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = format!("SET search_path TO {schema}, public");
    let pool = database::pool_options(3)
        .after_connect(move |conn, _| {
            let sql = search_path.clone();
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let v1 = insert_system(&pool, json!({"resourceType":"CodeSystem","url":"urn:codes","version":"1","content":"complete","concept":[{"code":"a","display":"First"},{"code":"b"}]})).await;
    let v2 = insert_system(&pool, json!({"resourceType":"CodeSystem","url":"urn:codes","version":"2","content":"complete","concept":[{"code":"a","display":"Second"},{"code":"c"}]})).await;
    let unversioned = insert_system(&pool, json!({"resourceType":"CodeSystem","url":"urn:codes","content":"complete","concept":[{"code":"unversioned","display":"Original"}]})).await;
    // Make the explicitly versioned release the default; instance calls must keep their row.
    sqlx::query("UPDATE code_systems SET updated_at = NOW() + interval '1 minute' WHERE id = $1")
        .bind(v2)
        .execute(&pool)
        .await
        .unwrap();
    let set = insert_set(&pool, Some("1"), json!({"include":[{"system":"urn:codes","version":"1"}],"exclude":[{"system":"urn:codes","version":"1","concept":[{"code":"b"}]}]})).await;
    insert_set(
        &pool,
        Some("2"),
        json!({"include":[{"system":"urn:codes"}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$lookup?system=urn:codes%7C1&code=a",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "version")["valueString"], "1");
    assert_eq!(parameter(&body, "display")["valueString"], "First");
    let (_, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{v1}/$lookup?code=a"),
        Value::Null,
    )
    .await;
    assert_eq!(parameter(&body, "version")["valueString"], "1");
    let (_, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{unversioned}/$validate-code?code=unversioned"),
        Value::Null,
    )
    .await;
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    let (_, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$validate-code?system=urn:codes&version=1&code=a&display=Wrong",
        Value::Null,
    )
    .await;
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    for path in [
        "/r4/CodeSystem/$lookup?system=urn:missing&code=a",
        "/r4/CodeSystem/$lookup?system=urn:codes&version=missing&code=a",
    ] {
        let (status, body) = request(&app, "GET", path, Value::Null).await;
        assert_eq!(status, 404);
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    let (status, _) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$lookup?system=urn:codes%7C1&version=2&code=a",
        Value::Null,
    )
    .await;
    assert_eq!(status, 422);
    for (code, expected) in [("a", true), ("b", false), ("c", false), ("missing", false)] {
        let (_,body) = request(&app,"GET",&format!("/r4/ValueSet/$validate-code?url=urn:set&valueSetVersion=1&system=urn:codes&code={code}"),Value::Null).await;
        assert_eq!(parameter(&body, "result")["valueBoolean"], expected);
    }
    let (_, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        json!({"resourceType":"Parameters","parameter":[
        {"name":"url","valueUri":"urn:set"},{"name":"valueSetVersion","valueString":"1"},
        {"name":"count","valueInteger":0},{"name":"offset","valueInteger":0}]}),
    )
    .await;
    assert_eq!(body["expansion"]["total"], 1);
    assert_eq!(body["expansion"]["contains"], json!([]));
    let (_, body) = request(
        &app,
        "GET",
        &format!("/r4/ValueSet/{set}/$expand?count=10"),
        Value::Null,
    )
    .await;
    assert_eq!(body["expansion"]["contains"][0]["code"], "a");
    let (status, _) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=urn:set&offset=-1",
        Value::Null,
    )
    .await;
    assert_eq!(status, 400);
    let (_, body) = request(&app, "GET", &format!("/r4/CodeSystem/{v1}"), Value::Null).await;
    assert_eq!(body["resourceType"], "CodeSystem");
    assert!(body.get("content").unwrap().is_string());
    // An atomic content/projection update is visible on the next request without a restart.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET content = jsonb_set(content, '{concept,0,display}', '\"Updated\"'), updated_at=NOW() WHERE id=$1").bind(v1).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE concepts SET display='Updated' WHERE code_system_id=$1 AND code='a'")
        .bind(v1)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let (_, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$lookup?system=urn:codes&version=1&code=a",
        Value::Null,
    )
    .await;
    assert_eq!(parameter(&body, "display")["valueString"], "Updated");
    let (_, body) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=urn:set&valueSetVersion=1",
        Value::Null,
    )
    .await;
    assert_eq!(body["expansion"]["contains"][0]["display"], "Updated");
    // No matching version must not fall back to the default ValueSet.
    let (_,body) = request(&app,"GET","/r4/ValueSet/$validate-code?url=urn:set&valueSetVersion=2&system=urn:codes&systemVersion=1&code=b",Value::Null).await;
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    let (_,body) = request(&app,"GET","/r4/ValueSet/$validate-code?url=urn:set&valueSetVersion=2&system=urn:codes&systemVersion=2&code=b",Value::Null).await;
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    let (status, _) = request(
        &app,
        "GET",
        "/r4/ValueSet/$expand?url=urn:set&valueSetVersion=missing",
        Value::Null,
    )
    .await;
    assert_eq!(status, 404);

    // Independent store/SQL sessions model a CLI writer. A reader keeps an immutable old plan.
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let mut readers = Vec::new();
    for _ in 0..16 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            store.expand_compose("urn:set", Some("1")).await.unwrap()
        }));
    }
    let mut plans = Vec::new();
    for reader in readers {
        plans.push(reader.await.unwrap());
    }
    for plan in &plans {
        assert!(Arc::ptr_eq(&plans[0].members, &plan.members));
    }
    // A cold request waiting for admission cannot block unrelated warmed requests.
    let paused = store.pause_preparation().await;
    let cold_store = store.clone();
    let cold =
        tokio::spawn(async move { cold_store.expand_compose("urn:set", Some("missing")).await });
    let warm = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        store.expand_compose("urn:set", Some("1")),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(Arc::ptr_eq(&warm.members, &plans[0].members));
    assert!(!cold.is_finished());
    cold.abort();
    assert!(matches!(cold.await, Err(error) if error.is_cancelled()));
    drop(paused);
    let old = plans.remove(0);
    assert!(old.members.find("urn:codes", Some("1"), "a").is_some());
    assert!(old.members.find("urn:codes", Some("1"), "new").is_none());
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET content=jsonb_set(content, '{concept}', '[{\"code\":\"new\"}]') WHERE id=$1")
        .bind(v1).execute(&mut *writer).await.unwrap();
    // Uncommitted writes must not invalidate the committed snapshot.
    let during = store.expand_compose("urn:set", Some("1")).await.unwrap();
    assert!(Arc::ptr_eq(&old.members, &during.members));
    // Remove the explicit exclusion of the now absent code in the same commit.
    sqlx::query("UPDATE value_sets SET content=content #- '{compose,exclude}' WHERE id=$1")
        .bind(set)
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.commit().await.unwrap();
    let new = store.expand_compose("urn:set", Some("1")).await.unwrap();
    assert!(!Arc::ptr_eq(&old.members, &new.members));
    assert!(new.members.find("urn:codes", Some("1"), "a").is_none());
    assert!(new.members.find("urn:codes", Some("1"), "new").is_some());
    assert!(old.members.find("urn:codes", Some("1"), "a").is_some());
    // Failed resolution is not a sticky negative cache entry.
    assert!(store.expand_compose("urn:set", Some("3")).await.is_err());
    let imported = insert_set(
        &pool,
        Some("3"),
        json!({"include":[{"valueSet":["urn:set|1"]}]}),
    )
    .await;
    let dependent = store.expand_compose("urn:set", Some("3")).await.unwrap();
    assert!(dependent.members.find("urn:codes", None, "new").is_some());
    sqlx::query("UPDATE value_sets SET content=jsonb_set(content, '{compose}', '{\"include\":[{\"system\":\"urn:codes\",\"version\":\"2\"}]}') WHERE id=$1")
        .bind(set).execute(&pool).await.unwrap();
    let replaced = store.expand_compose("urn:set", Some("3")).await.unwrap();
    assert!(replaced.members.find("urn:codes", Some("2"), "c").is_some());
    assert!(replaced.members.find("urn:codes", None, "new").is_none());
    // Rollback does not publish a generation or discard the prepared plan.
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM value_sets WHERE id=$1")
        .bind(imported)
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.rollback().await.unwrap();
    let retained = store.expand_compose("urn:set", Some("3")).await.unwrap();
    assert!(Arc::ptr_eq(&replaced.members, &retained.members));
    sqlx::query("DELETE FROM value_sets WHERE id=$1")
        .bind(imported)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.expand_compose("urn:set", Some("3")).await.is_err());
    let default_before = store.expand_compose("urn:set", Some("2")).await.unwrap();
    assert!(default_before
        .members
        .find("urn:codes", Some("2"), "c")
        .is_some());
    let v3 = insert_system(&pool, json!({"resourceType":"CodeSystem","url":"urn:codes","version":"3","content":"complete","concept":[{"code":"d"}]})).await;
    sqlx::query("UPDATE code_systems SET updated_at=NOW()+interval '2 minutes' WHERE id=$1")
        .bind(v3)
        .execute(&pool)
        .await
        .unwrap();
    let default_after = store.expand_compose("urn:set", Some("2")).await.unwrap();
    assert!(default_after
        .members
        .find("urn:codes", Some("3"), "d")
        .is_some());
    assert!(default_after.members.find("urn:codes", None, "c").is_none());
    let pinned = store
        .compose_for_validation("urn:set", Some("2"), None, "urn:codes", Some("2"))
        .await
        .unwrap();
    assert!(pinned.members.find("urn:codes", Some("2"), "c").is_some());
    // Entry pressure evicts plans; retained readers remain valid.
    let first = store.expand_compose("urn:set", Some("1")).await.unwrap();
    for version in 10..20 {
        let version = version.to_string();
        insert_set(
            &pool,
            Some(&version),
            json!({"include":[{"system":"urn:codes","version":"2"}]}),
        )
        .await;
    }
    let first = {
        let current = store.expand_compose("urn:set", Some("1")).await.unwrap();
        assert!(first.members.find("urn:codes", Some("2"), "c").is_some());
        current
    };
    for version in 10..20 {
        store
            .expand_compose("urn:set", Some(&version.to_string()))
            .await
            .unwrap();
    }
    let rebuilt = store.expand_compose("urn:set", Some("1")).await.unwrap();
    assert!(!Arc::ptr_eq(&first.members, &rebuilt.members));
    assert!(first.members.find("urn:codes", Some("2"), "c").is_some());
    // Concurrent resource/dependency updates must never produce a mixed-generation plan.
    let race_system = insert_system(&pool, json!({"url":"urn:race","version":"1","content":"complete","concept":[{"code":"x","display":"0"}]})).await;
    let race_set = insert_set(
        &pool,
        Some("race"),
        json!({"include":[{"system":"urn:race","version":"1"}]}),
    )
    .await;
    sqlx::query("UPDATE value_sets SET content=jsonb_set(content, '{title}', '\"0\"') WHERE id=$1")
        .bind(race_set)
        .execute(&pool)
        .await
        .unwrap();
    let writer_pool = pool.clone();
    let writer = tokio::spawn(async move {
        for generation in 1..=12 {
            let marker = sqlx::types::Json(json!(generation.to_string()));
            let mut tx = writer_pool.begin().await.unwrap();
            sqlx::query("UPDATE code_systems SET content=jsonb_set(content, '{concept,0,display}', $1) WHERE id=$2").bind(&marker).bind(race_system).execute(&mut *tx).await.unwrap();
            tokio::task::yield_now().await;
            sqlx::query(
                "UPDATE value_sets SET content=jsonb_set(content, '{title}', $1) WHERE id=$2",
            )
            .bind(&marker)
            .bind(race_set)
            .execute(&mut *tx)
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
    });
    let mut readers = Vec::new();
    for _ in 0..4 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            for _ in 0..40 {
                let plan = store.expand_compose("urn:set", Some("race")).await.unwrap();
                assert_eq!(
                    plan.members
                        .find("urn:race", Some("1"), "x")
                        .unwrap()
                        .display
                        .as_deref(),
                    plan.resource["title"].as_str()
                );
            }
        }));
    }
    writer.await.unwrap();
    for reader in readers {
        reader.await.unwrap();
    }
    let latest = store.expand_compose("urn:set", Some("race")).await.unwrap();
    assert_eq!(latest.resource["title"], "12");
    assert_eq!(
        latest
            .members
            .find("urn:race", Some("1"), "x")
            .unwrap()
            .display
            .as_deref(),
        Some("12")
    );
    // Exercise the production pool policy with a single, deliberately terminated socket.
    let recovery_search_path = format!("SET search_path TO {schema}, public");
    let recovery_pool = database::pool_options(1)
        .after_connect(move |conn, _| {
            let sql = recovery_search_path.clone();
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    let recovery_store = Arc::new(PostgresStore::new(recovery_pool.clone()));
    let recovery_app = api::create_router(recovery_store.clone());
    recovery_store
        .expand_compose("urn:set", Some("race"))
        .await
        .unwrap();
    for (marker, idle_check) in [("13", true), ("14", false)] {
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&recovery_pool)
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while recovery_pool.num_idle() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let mut writer = pool.begin().await.unwrap();
        sqlx::query("UPDATE code_systems SET content=jsonb_set(content, '{concept,0,display}', $1) WHERE id=$2")
            .bind(sqlx::types::Json(json!(marker))).bind(race_system).execute(&mut *writer).await.unwrap();
        sqlx::query("UPDATE value_sets SET content=jsonb_set(content, '{title}', $1) WHERE id=$2")
            .bind(sqlx::types::Json(json!(marker)))
            .bind(race_set)
            .execute(&mut *writer)
            .await
            .unwrap();
        writer.commit().await.unwrap();
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
                .bind(pid)
                .fetch_one(&pool)
                .await
                .unwrap()
        );
        if idle_check {
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        }
        let path = "/r4/ValueSet/$expand?url=urn:set&valueSetVersion=race";
        let (status, body) = request(&recovery_app, "GET", path, Value::Null).await;
        if !idle_check && status == 500 {
            // A socket dying inside the health interval may fail once, never return stale data.
            assert_eq!(body["resourceType"], "OperationOutcome");
            assert!(body.get("expansion").is_none());
        } else {
            assert_eq!(status, 200);
            assert_eq!(body["title"], marker);
            assert_eq!(body["expansion"]["contains"][0]["display"], marker);
        }
        let (status, body) = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            request(&recovery_app, "GET", path, Value::Null),
        )
        .await
        .unwrap();
        assert_eq!(status, 200);
        assert_eq!(body["title"], marker);
        assert_eq!(body["expansion"]["contains"][0]["display"], marker);
        let replacement: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&recovery_pool)
            .await
            .unwrap();
        assert_ne!(replacement, pid);
    }
    recovery_pool.close().await;
    sqlx::query("TRUNCATE value_sets CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.expand_compose("urn:set", Some("1")).await.is_err());
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn prepared_translation_versions_updates_and_concurrency() {
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = format!("SET search_path TO {schema}, public");
    let pool = database::pool_options(4)
        .after_connect(move |conn, _| {
            let sql = search_path.clone();
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let content = |version: &str, code: &str, display: &str| json!({"resourceType":"ConceptMap","url":"urn:map","version":version,"group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","display":"Source A","target":[{"code":code,"display":display,"equivalence":"wider"}]}]}]});
    let v1 = Uuid::new_v4();
    let v2 = Uuid::new_v4();
    for (id, version, code) in [(v1, "1", "b"), (v2, "2", "c")] {
        sqlx::query("INSERT INTO concept_maps(id,url,version,status,content) VALUES($1,'urn:map',$2,'active',$3)")
            .bind(id).bind(version).bind(sqlx::types::Json(content(version,code,code))).execute(&pool).await.unwrap();
    }
    sqlx::query("UPDATE concept_maps SET updated_at=NOW()+interval '1 minute' WHERE id=$1")
        .bind(v2)
        .execute(&pool)
        .await
        .unwrap();
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let mut readers = Vec::new();
    for _ in 0..16 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            store
                .prepare_concept_map("urn:map|1", None, None)
                .await
                .unwrap()
        }));
    }
    let mut plans = Vec::new();
    for reader in readers {
        plans.push(reader.await.unwrap());
    }
    for plan in &plans {
        assert!(Arc::ptr_eq(&plans[0], plan));
    }
    let old = plans.remove(0);
    // Even a warm plan must fail closed when ConceptMap write tracking is unavailable.
    sqlx::query("UPDATE terminology_generation SET maps_tracked=false WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .prepare_concept_map("urn:map|1", None, None)
        .await
        .is_err());
    sqlx::query("UPDATE terminology_generation SET maps_tracked=true WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(
        old.translate("urn:s", "a", None, false).unwrap()[0].code,
        "b"
    );
    assert!(old
        .translate("urn:s", "new", None, false)
        .unwrap()
        .is_empty());
    assert!(old
        .translate("urn:t", "future", None, true)
        .unwrap()
        .is_empty());
    assert!(store
        .prepare_concept_map("urn:map|1", Some("2"), None)
        .await
        .is_err());
    assert_eq!(
        store
            .prepare_concept_map("urn:map", None, None)
            .await
            .unwrap()
            .translate("urn:s", "a", None, false)
            .unwrap()[0]
            .code,
        "c"
    );
    assert_eq!(
        store
            .prepare_concept_map("urn:instance", None, Some(v1))
            .await
            .unwrap()
            .translate("urn:s", "a", None, false)
            .unwrap()[0]
            .code,
        "b"
    );
    let mut writer = pool.begin().await.unwrap();
    let updated = json!({"resourceType":"ConceptMap","url":"urn:map","version":"1","group":[{"source":"urn:s","target":"urn:t","element":[{"code":"new","display":"New source","target":[{"code":"future","display":"New target"}]}]}]});
    sqlx::query("UPDATE concept_maps SET content=$1 WHERE id=$2")
        .bind(sqlx::types::Json(&updated))
        .bind(v1)
        .execute(&mut *writer)
        .await
        .unwrap();
    let during = store
        .prepare_concept_map("urn:map", Some("1"), None)
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&during, &old));
    writer.commit().await.unwrap();
    let fresh = store
        .prepare_concept_map("urn:map", Some("1"), None)
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&fresh, &old));
    assert!(fresh
        .translate("urn:s", "a", None, false)
        .unwrap()
        .is_empty());
    assert_eq!(
        fresh.translate("urn:s", "new", None, false).unwrap()[0].code,
        "future"
    );
    assert_eq!(
        fresh.translate("urn:t", "future", None, true).unwrap()[0].code,
        "new"
    );
    assert_eq!(
        old.translate("urn:s", "a", None, false).unwrap()[0].code,
        "b"
    );
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("DELETE FROM concept_maps WHERE id=$1")
        .bind(v1)
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.rollback().await.unwrap();
    assert!(Arc::ptr_eq(
        &fresh,
        &store
            .prepare_concept_map("urn:map", Some("1"), None)
            .await
            .unwrap()
    ));
    assert!(store
        .prepare_concept_map("urn:map", Some("3"), None)
        .await
        .is_err());
    let v3 = Uuid::new_v4();
    sqlx::query("INSERT INTO concept_maps(id,url,version,status,content,updated_at) VALUES($1,'urn:map','3','active',$2,NOW()+interval '2 minutes')")
        .bind(v3).bind(sqlx::types::Json(content("3","d","d"))).execute(&pool).await.unwrap();
    assert_eq!(
        store
            .prepare_concept_map("urn:map", None, None)
            .await
            .unwrap()
            .translate("urn:s", "a", None, false)
            .unwrap()[0]
            .code,
        "d"
    );
    let app = api::create_router(store.clone());
    let (status, body) = request(
        &app,
        "GET",
        "/r4/ConceptMap/$translate?url=urn:map%7C1&system=urn:t&code=future&reverse=true",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    assert_eq!(
        body["parameter"][1]["part"][1]["valueCoding"]["code"],
        "new"
    );
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/ConceptMap/{v1}/$translate?system=urn:s&code=new"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    let (status,body)=request(&app,"POST","/r4/ConceptMap/$translate",json!({"resourceType":"Parameters","parameter":[{"name":"url","valueUri":"urn:map"},{"name":"conceptMapVersion","valueString":"2"},{"name":"system","valueUri":"urn:s"},{"name":"code","valueCode":"a"}]})).await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    assert_eq!(body["parameter"][1]["part"][1]["valueCoding"]["code"], "c");
    // Pressure a fixed entry budget without mutating the generation between reads.
    for version in 10..20 {
        let version = version.to_string();
        sqlx::query("INSERT INTO concept_maps(id,url,version,status,content) VALUES($1,'urn:map',$2,'active',$3)").bind(Uuid::new_v4()).bind(&version).bind(sqlx::types::Json(content(&version,"x","X"))).execute(&pool).await.unwrap();
    }
    let retained = store
        .prepare_concept_map("urn:map", Some("1"), None)
        .await
        .unwrap();
    for version in 10..20 {
        store
            .prepare_concept_map("urn:map", Some(&version.to_string()), None)
            .await
            .unwrap();
    }
    let rebuilt = store
        .prepare_concept_map("urn:map", Some("1"), None)
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&retained, &rebuilt));
    assert_eq!(
        retained.translate("urn:s", "new", None, false).unwrap()[0].code,
        "future"
    );
    sqlx::query("DELETE FROM concept_maps WHERE id=$1")
        .bind(v1)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .prepare_concept_map("urn:map", Some("1"), None)
        .await
        .is_err());
    sqlx::query("TRUNCATE concept_maps CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .prepare_concept_map("urn:map", None, None)
        .await
        .is_err());
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn hierarchy_test_database() -> (PgPool, PgPool, String) {
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let search_path = format!("SET search_path TO {schema}, public");
    let pool = database::pool_options(5)
        .after_connect(move |conn, _| {
            let sql = search_path.clone();
            Box::pin(async move {
                sqlx::query(&sql).execute(conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    (admin, pool, schema)
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn hierarchy_projection_freshness_admission_and_eviction() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut ids = Vec::new();
    for version in 0..10 {
        ids.push(insert_system(&pool, json!({"url":"urn:prepared-hierarchy","version":version.to_string(),"content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"A","concept":[{"code":"B"}]}]})).await);
    }
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let mut readers = Vec::new();
    for _ in 0..16 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            store
                .resolve_subsumption("urn:prepared-hierarchy", Some("0"), "A", "B")
                .await
                .unwrap()
                .unwrap()
        }));
    }
    for reader in readers {
        assert_eq!(reader.await.unwrap().relationship, Some(true));
    }
    assert_eq!(store.hierarchy_cache_usage().await.0, 1);
    let paused = store.pause_hierarchy_preparation().await;
    let cold_store = store.clone();
    let cold = tokio::spawn(async move {
        cold_store
            .resolve_subsumption("urn:prepared-hierarchy", Some("1"), "A", "B")
            .await
    });
    let warm = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        store.resolve_subsumption("urn:prepared-hierarchy", Some("0"), "A", "B"),
    )
    .await
    .unwrap()
    .unwrap()
    .unwrap();
    assert_eq!(warm.relationship, Some(true));
    assert!(!cold.is_finished());
    cold.abort();
    assert!(matches!(cold.await, Err(error) if error.is_cancelled()));
    drop(paused);
    for version in 1..10 {
        assert_eq!(
            store
                .resolve_subsumption(
                    "urn:prepared-hierarchy",
                    Some(&version.to_string()),
                    "A",
                    "B"
                )
                .await
                .unwrap()
                .unwrap()
                .relationship,
            Some(true)
        );
        let (entries, bytes) = store.hierarchy_cache_usage().await;
        assert!(entries <= 8 && bytes <= 16 * 1024 * 1024);
    }
    assert_eq!(warm.code_a.as_deref(), Some("A"));
    assert_eq!(
        store
            .resolve_subsumption_by_id(&ids[0], "A", "B")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    // Only the closure projection changes; content JSON is deliberately untouched.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE closure_table SET ancestor_code='B',descendant_code='A' WHERE code_system_id=$1",
    )
    .bind(ids[0])
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&ids[0], "A", "B")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&ids[0], "A", "B")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    sqlx::query(
        "UPDATE closure_table SET ancestor_code='B',descendant_code='A' WHERE code_system_id=$1",
    )
    .bind(ids[0])
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&ids[0], "A", "B")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(false)
    );
    // A cached missing code must disappear after a projection-only insert.
    assert!(store
        .resolve_subsumption_by_id(&ids[0], "C", "B")
        .await
        .unwrap()
        .unwrap()
        .code_a
        .is_none());
    sqlx::query("INSERT INTO concepts(code_system_id,code) VALUES($1,'C')")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&ids[0], "C", "B")
            .await
            .unwrap()
            .unwrap()
            .code_a
            .as_deref(),
        Some("C")
    );
    sqlx::query("UPDATE concepts SET code='D' WHERE code_system_id=$1 AND code='C'")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&ids[0], "C", "B")
        .await
        .unwrap()
        .unwrap()
        .code_a
        .is_none());
    assert_eq!(
        store
            .resolve_subsumption_by_id(&ids[0], "D", "B")
            .await
            .unwrap()
            .unwrap()
            .code_a
            .as_deref(),
        Some("D")
    );
    sqlx::query("UPDATE terminology_generation SET hierarchies_tracked=false")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&ids[0], "A", "B")
        .await
        .is_err());
    sqlx::query("UPDATE terminology_generation SET hierarchies_tracked=true")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption("urn:prepared-hierarchy", Some("new"), "A", "B")
        .await
        .unwrap()
        .is_none());
    let new = insert_system(&pool, json!({"url":"urn:prepared-hierarchy","version":"new","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"B","concept":[{"code":"A"}]}]})).await;
    sqlx::query("UPDATE code_systems SET updated_at=NOW()+interval '1 minute' WHERE id=$1")
        .bind(new)
        .execute(&pool)
        .await
        .unwrap();
    for version in [None, Some("new")] {
        assert_eq!(
            store
                .resolve_subsumption("urn:prepared-hierarchy", version, "A", "B")
                .await
                .unwrap()
                .unwrap()
                .relationship,
            Some(false)
        );
    }
    sqlx::query("TRUNCATE closure_table")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&new, "A", "B")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        None
    );
    sqlx::query("TRUNCATE concepts")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&new, "A", "B")
        .await
        .unwrap()
        .unwrap()
        .code_a
        .is_none());
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(new)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&new, "A", "B")
        .await
        .unwrap()
        .is_none());
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn hierarchy_large_folded_projection_and_sql_admission() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut ids = Vec::new();
    // A fragment is served from existing projections, not reparsed as a complete resource.
    for (url, children) in [("urn:folded", 10_000_i32), ("urn:over-limit", 100_000)] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO code_systems(id,url,status,content) VALUES($1,$2,'active',$3)")
            .bind(id).bind(url).bind(sqlx::types::Json(json!({"resourceType":"CodeSystem","url":url,"content":"fragment","caseSensitive":false,"hierarchyMeaning":"is-a"}))).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO concepts(code_system_id,code) SELECT $1,CASE WHEN n=0 THEN 'ROOT' ELSE 'C' || lpad(n::text,6,'0') END FROM generate_series(0,$2) n")
            .bind(id).bind(children).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO closure_table(code_system_id,ancestor_code,descendant_code,depth) SELECT $1,'ROOT','C' || lpad(n::text,6,'0'),1 FROM generate_series(1,$2) n")
            .bind(id).bind(children).execute(&pool).await.unwrap();
        ids.push(id);
    }
    let store = PostgresStore::new(pool.clone());
    for id in &ids {
        for _ in 0..3 {
            for (a, b, stored_a, stored_b, relationship) in [
                ("root", "c010000", "ROOT", "C010000", Some(true)),
                ("C010000", "root", "C010000", "ROOT", Some(false)),
                ("c010000", "C010000", "C010000", "C010000", None),
                ("c000001", "c010000", "C000001", "C010000", None),
            ] {
                let result = store
                    .resolve_subsumption_by_id(id, a, b)
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(result.code_a.as_deref(), Some(stored_a));
                assert_eq!(result.code_b.as_deref(), Some(stored_b));
                assert_eq!(result.relationship, relationship);
            }
        }
        assert!(store
            .resolve_subsumption_by_id(id, "missing", "root")
            .await
            .unwrap()
            .unwrap()
            .code_a
            .is_none());
        assert!(store
            .resolve_subsumption_by_id(id, &"a".repeat(4097), "root")
            .await
            .unwrap()
            .unwrap()
            .code_a
            .is_none());
    }
    // One retained index and one generation-scoped SQL admission marker.
    let (entries, bytes) = store.hierarchy_cache_usage().await;
    assert_eq!(entries, 2);
    assert!(bytes < 16 * 1024 * 1024);
    sqlx::query("DELETE FROM closure_table WHERE code_system_id=$1 AND descendant_code='C010000'")
        .bind(ids[1])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&ids[1], "root", "c010000")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        None
    );
    // PostgreSQL supplies both stored and query keys, including non-ASCII folding.
    sqlx::query("INSERT INTO concepts(code_system_id,code) VALUES($1,'İ')")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO closure_table(code_system_id,ancestor_code,descendant_code,depth) VALUES($1,'ROOT','İ',1)").bind(ids[0]).execute(&pool).await.unwrap();
    let folded: String = sqlx::query_scalar("SELECT lower('İ')")
        .fetch_one(&pool)
        .await
        .unwrap();
    for probe in ["İ", folded.as_str()] {
        let result = store
            .resolve_subsumption_by_id(&ids[0], "root", probe)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.code_b.as_deref(), Some("İ"));
        assert_eq!(result.relationship, Some(true));
    }
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn wait_generation_stats(
    store: &PostgresStore,
    condition: impl Fn((usize, usize, usize)) -> bool,
) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if condition(store.generation_stats()) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("generation synchronization timed out");
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn generation_batches_exclude_requests_after_their_sql_snapshot() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = insert_system(&pool, json!({"url":"urn:generation","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[{"code":"ROOT","concept":[{"code":"A"},{"code":"B"}]}]})).await;
    let set = insert_set(
        &pool,
        None,
        json!({"include":[{"system":"urn:generation"}]}),
    )
    .await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    // Warm a plan before observing the boundary, so the old operation can use it.
    store
        .resolve_subsumption_by_id(&id, "root", "a")
        .await
        .unwrap();
    let completed = store.generation_stats().1;
    let delivery = store.pause_generation(true).await;
    let old_store = store.clone();
    let old =
        tokio::spawn(async move { old_store.resolve_subsumption_by_id(&id, "root", "a").await });
    wait_generation_stats(&store, |stats| stats.1 > completed).await;
    // The old query has completed, but no reply has been delivered yet.
    sqlx::query("DELETE FROM closure_table WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let new_store = store.clone();
    let new =
        tokio::spawn(async move { new_store.resolve_subsumption_by_id(&id, "root", "a").await });
    wait_generation_stats(&store, |stats| stats.2 == 1).await;
    assert!(!new.is_finished());
    drop(delivery);
    assert_eq!(
        old.await.unwrap().unwrap().unwrap().relationship,
        Some(true)
    );
    assert_eq!(new.await.unwrap().unwrap().unwrap().relationship, None);
    assert_eq!(store.generation_stats().1, completed + 2);

    // A cancelled recipient of an already executed batch cannot strand the next one.
    let completed = store.generation_stats().1;
    let delivery = store.pause_generation(true).await;
    let cancelled_store = store.clone();
    let cancelled = tokio::spawn(async move {
        cancelled_store
            .resolve_subsumption_by_id(&id, "root", "a")
            .await
    });
    wait_generation_stats(&store, |stats| stats.1 > completed).await;
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    drop(delivery);
    assert_eq!(
        store
            .resolve_subsumption_by_id(&id, "root", "a")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        None
    );

    // Mixed queries share one statement; each folded key must keep its recipient.
    let dispatch = store.pause_generation(false).await;
    let started = store.generation_stats().0;
    let delivery = store.pause_generation(true).await;
    let mut readers = Vec::new();
    for i in 0..32 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            let (a, b) = if i % 2 == 0 {
                ("root", "a")
            } else {
                ("b", "root")
            };
            let result = store
                .resolve_subsumption_by_id(&id, a, b)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(
                result.code_a.as_deref(),
                Some(if i % 2 == 0 { "ROOT" } else { "B" })
            );
            assert_eq!(
                result.code_b.as_deref(),
                Some(if i % 2 == 0 { "A" } else { "ROOT" })
            );
        }));
    }
    let set_store = store.clone();
    let membership = tokio::spawn(async move { set_store.expand_compose("urn:set", None).await });
    wait_generation_stats(&store, |stats| stats.2 == 33).await;
    drop(dispatch);
    wait_generation_stats(&store, |stats| stats.1 > started).await;
    let cancelled = readers.remove(0);
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    drop(delivery);
    for reader in readers {
        reader.await.unwrap();
    }
    assert_eq!(
        membership.await.unwrap().unwrap().members.values().count(),
        3
    );
    assert_eq!(store.generation_stats().0, started + 1);

    // SQL failure cannot return a warmed plan, and does not poison future observations.
    sqlx::query(
        "ALTER TABLE terminology_generation RENAME COLUMN generation TO unavailable_generation",
    )
    .execute(&pool)
    .await
    .unwrap();
    let app = api::create_router(store.clone());
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{id}/$subsumes?codeA=root&codeB=a"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 500);
    assert_eq!(body["resourceType"], "OperationOutcome");
    sqlx::query(
        "ALTER TABLE terminology_generation RENAME COLUMN unavailable_generation TO generation",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&id, "root", "a")
        .await
        .is_ok());
    sqlx::query("UPDATE terminology_generation SET maps_tracked=false,hierarchies_tracked=false")
        .execute(&pool)
        .await
        .unwrap();
    let dispatch = store.pause_generation(false).await;
    let hierarchy_store = store.clone();
    let hierarchy = tokio::spawn(async move {
        hierarchy_store
            .resolve_subsumption_by_id(&id, "root", "a")
            .await
    });
    let map_store = store.clone();
    let map = tokio::spawn(async move {
        map_store
            .prepare_concept_map("urn:missing", None, None)
            .await
    });
    let set_store = store.clone();
    let membership = tokio::spawn(async move { set_store.expand_compose("urn:set", None).await });
    wait_generation_stats(&store, |stats| stats.2 == 3).await;
    drop(dispatch);
    assert!(hierarchy.await.unwrap().is_err());
    assert!(map.await.unwrap().is_err());
    assert_eq!(
        membership.await.unwrap().unwrap().members.values().count(),
        3
    );
    sqlx::query("UPDATE terminology_generation SET maps_tracked=true,hierarchies_tracked=true")
        .execute(&pool)
        .await
        .unwrap();
    // A ValueSet write still invalidates a warmed compose result.
    sqlx::query("UPDATE value_sets SET content=jsonb_set(content,'{compose}', '{\"include\":[{\"system\":\"urn:generation\",\"concept\":[{\"code\":\"A\"}]}]}') WHERE id=$1").bind(set).execute(&pool).await.unwrap();
    assert_eq!(
        store
            .expand_compose("urn:set", None)
            .await
            .unwrap()
            .members
            .values()
            .count(),
        1
    );
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn generation_queue_is_bounded_and_cancelled_waiters_release_capacity() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = insert_system(&pool, json!({"url":"urn:queue","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"A","concept":[{"code":"B"}]}]})).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    store
        .resolve_subsumption_by_id(&id, "A", "B")
        .await
        .unwrap();
    let dispatch = store.pause_generation(false).await;
    let started = store.generation_stats().0;
    let mut readers = Vec::new();
    for _ in 0..256 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            store.resolve_subsumption_by_id(&id, "A", "B").await
        }));
    }
    wait_generation_stats(&store, |stats| stats.2 == 256).await;
    let app = api::create_router(store.clone());
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{id}/$subsumes?codeA=A&codeB=B"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(body["resourceType"], "OperationOutcome");
    assert_eq!(store.generation_stats().0, started);
    let cancelled = readers.pop().unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    let next_store = store.clone();
    let next =
        tokio::spawn(async move { next_store.resolve_subsumption_by_id(&id, "B", "A").await });
    // Admission removes the cancelled sender and occupies the released slot.
    wait_generation_stats(&store, |stats| stats.2 == 256).await;
    drop(dispatch);
    for reader in readers {
        assert_eq!(
            reader.await.unwrap().unwrap().unwrap().relationship,
            Some(true)
        );
    }
    assert_eq!(
        next.await.unwrap().unwrap().unwrap().relationship,
        Some(false)
    );
    // An idle worker must exit without retaining a completed observation or pool slot.
    assert_eq!(
        store
            .resolve_subsumption_by_id(&id, "A", "B")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    pool.close().await;
    assert!(store
        .resolve_subsumption_by_id(&id, "A", "B")
        .await
        .is_err());
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn prepared_concept_versions_payloads_freshness_and_admission() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut ids = Vec::new();
    for version in 0..10 {
        ids.push(insert_system(&pool,json!({"url":"urn:concept-plan","version":version.to_string(),"content":"complete","caseSensitive":false,"concept":[{"code":"A","display":version.to_string()}]})).await);
    }
    let unversioned = insert_system(&pool,json!({"url":"urn:concept-plan","content":"complete","concept":[{"code":"original","display":"Original"}]})).await;
    sqlx::query("UPDATE code_systems SET updated_at=NOW()+interval '1 minute' WHERE id=$1")
        .bind(ids[9])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE concepts SET definition='Definition',properties='{\"rank\":7,\"flag\":true}' WHERE code_system_id=$1").bind(ids[0]).execute(&pool).await.unwrap();
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    for (context, code, display, version) in [
        (
            "$lookup?system=urn:concept-plan&version=0".to_string(),
            "a",
            "0",
            Some("0"),
        ),
        (
            "$lookup?system=urn:concept-plan%7C0".to_string(),
            "A",
            "0",
            Some("0"),
        ),
        (
            "$lookup?system=urn:concept-plan".to_string(),
            "a",
            "9",
            Some("9"),
        ),
        (
            format!("{unversioned}/$lookup?"),
            "original",
            "Original",
            None,
        ),
    ] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/{context}&code={code}"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "display")["valueString"], display);
        if let Some(version) = version {
            assert_eq!(parameter(&body, "version")["valueString"], version);
        } else {
            assert!(!body["parameter"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["name"] == "version"));
        }
    }
    let stored = store
        .resolve_concept_by_id(&ids[0], "a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.code.as_deref(), Some("A"));
    assert_eq!(stored.definition.as_deref(), Some("Definition"));
    assert_eq!(
        stored.properties.as_ref().unwrap().0,
        json!({"rank":7,"flag":true})
    );
    let (status, body) = request(
        &app,
        "POST",
        &format!("/r4/CodeSystem/{}/$lookup", ids[0]),
        json!({"resourceType":"Parameters","parameter":[{"name":"code","valueCode":"a"}]}),
    )
    .await;
    assert_eq!(status, 200);
    assert!(body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "property"
            && p["part"][0]["valueCode"] == "definition"
            && p["part"][1]["valueString"] == "Definition"));
    assert!(
        !body["parameter"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "designation"),
        "Definitions must not be represented as invented designation uses"
    );
    assert!(body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "property"
            && p["part"][0]["valueCode"] == "rank"
            && p["part"][1]["valueString"] == "7"));
    for (operation, display, expected) in [
        ("$validate-code", "0", true),
        ("$validate-code", "Wrong", false),
    ] {
        let (status,body) = request(&app,"POST",&format!("/r4/CodeSystem/{operation}"),json!({"resourceType":"Parameters","parameter":[{"name":"system","valueUri":"urn:concept-plan"},{"name":"version","valueString":"0"},{"name":"code","valueCode":"a"},{"name":"display","valueString":display}]})).await;
        assert_eq!(status, 200);
        assert_eq!(parameter(&body, "result")["valueBoolean"], expected);
        assert_eq!(parameter(&body, "display")["valueString"], "0");
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$validate-code?system=urn:concept-plan&version=0&code=missing",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    for (path, status) in [
        (
            "/r4/CodeSystem/$lookup?system=urn:concept-plan&version=0&code=missing",
            404,
        ),
        (
            "/r4/CodeSystem/$lookup?system=urn:concept-plan%7C0&version=1&code=A",
            422,
        ),
        ("/r4/CodeSystem/$lookup?system=urn:missing&code=A", 404),
    ] {
        let (got, body) = request(&app, "GET", path, Value::Null).await;
        assert_eq!(got, status);
        assert_eq!(body["resourceType"], "OperationOutcome");
    }

    // Warm cache hits bypass cold preparation, including a cancelled cold waiter.
    let paused = store.pause_concept_preparation().await;
    let cold_store = store.clone();
    let cold = tokio::spawn(async move {
        cold_store
            .resolve_concept("urn:concept-plan", Some("1"), "a")
            .await
    });
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(2),
            store.resolve_concept_by_id(&ids[0], "a")
        )
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .display
        .as_deref(),
        Some("0")
    );
    assert!(!cold.is_finished());
    cold.abort();
    assert!(cold.await.unwrap_err().is_cancelled());
    drop(paused);
    for version in 0..10 {
        assert!(store
            .resolve_concept("urn:concept-plan", Some(&version.to_string()), "a")
            .await
            .unwrap()
            .unwrap()
            .code
            .is_some());
        let (entries, bytes) = store.concept_cache_usage().await;
        assert!(entries <= 8 && bytes <= 32 * 1024 * 1024);
    }
    assert_eq!(stored.definition.as_deref(), Some("Definition"));
    // Projection-only updates are transactional, not reconstructed from resource JSON.
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE concepts SET display='Changed' WHERE code_system_id=$1")
        .bind(ids[0])
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_concept_by_id(&ids[0], "a")
            .await
            .unwrap()
            .unwrap()
            .display
            .as_deref(),
        Some("0")
    );
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE concepts SET display='Changed' WHERE code_system_id=$1")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_concept_by_id(&ids[0], "a")
            .await
            .unwrap()
            .unwrap()
            .display
            .as_deref(),
        Some("Changed")
    );
    assert!(store
        .resolve_concept_by_id(&ids[0], "new")
        .await
        .unwrap()
        .unwrap()
        .code
        .is_none());
    sqlx::query("INSERT INTO concepts(code_system_id,code,display) VALUES($1,'NEW','Inserted')")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_concept_by_id(&ids[0], "new")
            .await
            .unwrap()
            .unwrap()
            .code
            .as_deref(),
        Some("NEW")
    );
    sqlx::query(
        "UPDATE code_systems SET content=jsonb_set(content,'{caseSensitive}','true') WHERE id=$1",
    )
    .bind(ids[0])
    .execute(&pool)
    .await
    .unwrap();
    assert!(store
        .resolve_concept_by_id(&ids[0], "a")
        .await
        .unwrap()
        .unwrap()
        .code
        .is_none());
    assert!(store
        .resolve_concept_by_id(&ids[0], "A")
        .await
        .unwrap()
        .unwrap()
        .code
        .is_some());
    sqlx::query("UPDATE code_systems SET content=jsonb_set(content,'{caseSensitive}','false'),name='0' WHERE id=$1").bind(ids[0]).execute(&pool).await.unwrap();
    sqlx::query("UPDATE concepts SET display='0' WHERE code_system_id=$1")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    let writer_pool = pool.clone();
    let id = ids[0];
    let writer = tokio::spawn(async move {
        for n in 1..=16 {
            let marker = n.to_string();
            let mut tx = writer_pool.begin().await.unwrap();
            sqlx::query("UPDATE code_systems SET name=$1 WHERE id=$2")
                .bind(&marker)
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
            tokio::task::yield_now().await;
            sqlx::query("UPDATE concepts SET display=$1 WHERE code_system_id=$2")
                .bind(&marker)
                .bind(id)
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
    });
    let mut readers = Vec::new();
    for _ in 0..3 {
        let store = store.clone();
        readers.push(tokio::spawn(async move {
            for _ in 0..32 {
                let result = store
                    .resolve_concept_by_id(&id, "a")
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(result.name, result.display);
            }
        }));
    }
    writer.await.unwrap();
    for reader in readers {
        reader.await.unwrap();
    }
    assert_eq!(
        store
            .resolve_concept_by_id(&id, "a")
            .await
            .unwrap()
            .unwrap()
            .display
            .as_deref(),
        Some("16")
    );
    sqlx::query("UPDATE terminology_generation SET hierarchies_tracked=false")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.resolve_concept_by_id(&id, "a").await.is_err());
    sqlx::query("UPDATE terminology_generation SET hierarchies_tracked=true")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE code_systems SET content=jsonb_set(content,'{content}','\"fragment\"') WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    for op in ["$lookup", "$validate-code"] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/{id}/{op}?code=absent"),
            Value::Null,
        )
        .await;
        assert_eq!(status, 400);
        assert_eq!(body["resourceType"], "OperationOutcome");
        assert_eq!(
            request(
                &app,
                "GET",
                &format!("/r4/CodeSystem/{id}/{op}?code=a"),
                Value::Null
            )
            .await
            .0,
            200
        );
    }
    assert!(store
        .resolve_concept("urn:concept-plan", Some("later"), "x")
        .await
        .unwrap()
        .is_none());
    let later = insert_system(&pool,json!({"url":"urn:concept-plan","version":"later","content":"complete","concept":[{"code":"x","display":"Later"}]})).await;
    sqlx::query("UPDATE code_systems SET updated_at=NOW()+interval '2 minutes' WHERE id=$1")
        .bind(later)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_concept("urn:concept-plan", None, "x")
            .await
            .unwrap()
            .unwrap()
            .display
            .as_deref(),
        Some("Later")
    );
    sqlx::query("TRUNCATE concepts")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_concept_by_id(&later, "x")
        .await
        .unwrap()
        .unwrap()
        .code
        .is_none());
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(later)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_concept_by_id(&later, "x")
        .await
        .unwrap()
        .is_none());
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn prepared_concept_large_folded_and_bounded_sql_fallback() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let mut ids = Vec::new();
    for (url, count) in [
        ("urn:large-concepts", 10001_i32),
        ("urn:concept-count-limit", 100001),
        ("urn:property-limit", 1),
        ("urn:decoded-limit", 400),
        ("urn:metadata-limit", 1),
    ] {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO code_systems(id,url,status,content) VALUES($1,$2,'active',$3)")
            .bind(id)
            .bind(url)
            .bind(sqlx::types::Json(
                json!({"url":url,"content":"fragment","caseSensitive":false}),
            ))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO concepts(code_system_id,code,display) SELECT $1,'C'||lpad(n::text,6,'0'),'Shown '||n::text FROM generate_series(1,$2) n")
            .bind(id).bind(count).execute(&pool).await.unwrap();
        ids.push(id);
    }
    sqlx::query("INSERT INTO concepts(code_system_id,code,display) VALUES($1,'İ','Unicode')")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE concepts SET properties=jsonb_build_object('blob',repeat('x',70000)) WHERE code_system_id=$1").bind(ids[2]).execute(&pool).await.unwrap();
    sqlx::query("UPDATE concepts SET properties=(SELECT jsonb_agg('{}'::jsonb) FROM generate_series(1,2000)) WHERE code_system_id=$1").bind(ids[3]).execute(&pool).await.unwrap();
    sqlx::query("UPDATE code_systems SET content=jsonb_set(content,'{content}',to_jsonb(repeat('x',70000))) WHERE id=$1").bind(ids[4]).execute(&pool).await.unwrap();
    let store = PostgresStore::new(pool.clone());
    for code in ["C010001", "c010001"] {
        let found = store
            .resolve_concept_by_id(&ids[0], code)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.code.as_deref(), Some("C010001"));
        assert_eq!(found.display.as_deref(), Some("Shown 10001"));
    }
    let folded: String = sqlx::query_scalar("SELECT lower('İ')")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .resolve_concept_by_id(&ids[0], &folded)
            .await
            .unwrap()
            .unwrap()
            .display
            .as_deref(),
        Some("Unicode")
    );
    let (entries, bytes) = store.concept_cache_usage().await;
    assert_eq!(entries, 1);
    assert!(bytes > 100_000 && bytes < 32 * 1024 * 1024);
    assert!(store
        .resolve_concept_by_id(&ids[0], &"a".repeat(4097))
        .await
        .unwrap()
        .unwrap()
        .code
        .is_none());
    for (i, id) in ids.iter().enumerate().skip(1) {
        let store = PostgresStore::new(pool.clone());
        for _ in 0..2 {
            let found = store
                .resolve_concept_by_id(id, "c000001")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(found.code.as_deref(), Some("C000001"));
            match i {
                2 => assert_eq!(
                    found.properties.unwrap().0["blob"].as_str().unwrap().len(),
                    70000
                ),
                3 => assert_eq!(found.properties.unwrap().0.as_array().unwrap().len(), 2000),
                4 => assert_eq!(found.content_mode.unwrap().len(), 70000),
                _ => {}
            }
        }
        // Count/property/decoded-memory/metadata rejection retains only a small SQL marker.
        let (entries, bytes) = store.concept_cache_usage().await;
        assert_eq!(entries, 1);
        assert!(bytes < 65536, "{i}: {bytes}");
        assert!(store
            .resolve_concept_by_id(id, "missing")
            .await
            .unwrap()
            .unwrap()
            .code
            .is_none());
        sqlx::query(
            "UPDATE concepts SET display='Fresh' WHERE code_system_id=$1 AND code='C000001'",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            store
                .resolve_concept_by_id(id, "c000001")
                .await
                .unwrap()
                .unwrap()
                .display
                .as_deref(),
            Some("Fresh")
        );
    }
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

fn inline_parameters(resource: Value, extra: Vec<Value>) -> Value {
    let mut parameters = vec![json!({"name":"valueSet","resource":resource})];
    parameters.extend(extra);
    json!({"resourceType":"Parameters","parameter":parameters})
}

#[tokio::test]
async fn inline_valueset_rejects_invalid_selectors_without_database() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let resource =
        json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:codes"}]}});
    let base = inline_parameters(resource.clone(), vec![]);
    let mut invalid = vec![
        inline_parameters(json!({"resourceType":"CodeSystem"}), vec![]),
        inline_parameters(json!({"resourceType":"ValueSet"}), vec![]),
        inline_parameters(json!({"resourceType":"ValueSet","compose":[]}), vec![]),
        inline_parameters(
            resource.clone(),
            vec![json!({"name":"url","valueUri":"urn:set"})],
        ),
        inline_parameters(
            resource.clone(),
            vec![json!({"name":"valueSet","resource":resource})],
        ),
        inline_parameters(
            json!({"resourceType":"ValueSet","version":"1","compose":{}}),
            vec![json!({"name":"valueSetVersion","valueString":"2"})],
        ),
        inline_parameters(
            json!({"resourceType":"ValueSet","url":7,"compose":{}}),
            vec![],
        ),
        inline_parameters(
            json!({"resourceType":"ValueSet","url":"","compose":{}}),
            vec![],
        ),
        inline_parameters(
            json!({"resourceType":"ValueSet","version":null,"compose":{}}),
            vec![],
        ),
        inline_parameters(
            json!({"resourceType":"ValueSet","compose":{}}),
            vec![json!({"name":"valueSetVersion","valueInteger":1})],
        ),
        inline_parameters(
            json!({"resourceType":"ValueSet","description":"x".repeat(256*1024),"compose":{}}),
            vec![],
        ),
        json!({"resourceType":"Parameters","parameter":[{"name":"valueSet","valueString":"not a resource"}]}),
    ];
    for parameter in [
        json!({"name":"count","valueString":"1"}),
        json!({"name":"filter","valueInteger":1}),
        json!({"name":"systemVersion","valueInteger":1}),
        json!({"name":"includeDesignations","valueString":"true"}),
        json!({"name":"coding","valueCoding":{"system":"urn:codes","code":"A"}}),
    ] {
        let mut input = base.clone();
        input["parameter"].as_array_mut().unwrap().push(parameter);
        invalid.push(input);
    }
    let mut mixed = base.clone();
    mixed["parameter"][0]["valueString"] = "ambiguous".into();
    invalid.push(mixed);
    let mut double_value = base.clone();
    double_value["parameter"][0]["valueString"] = "ambiguous".into();
    double_value["parameter"][0]["valueBoolean"] = true.into();
    invalid.push(double_value);
    let mut unknown_value = base.clone();
    unknown_value["parameter"][0]["valueDateTime"] = "2026-11-03".into();
    invalid.push(unknown_value);
    let mut parts = base.clone();
    parts["parameter"][0]["part"] = json!([]);
    invalid.push(parts);
    let mut wrong_envelope = base.clone();
    wrong_envelope["resourceType"] = "ValueSet".into();
    invalid.push(wrong_envelope);
    let mut duplicate = base.clone();
    duplicate["parameter"].as_array_mut().unwrap().extend([
        json!({"name":"valueSetVersion","valueString":"1"}),
        json!({"name":"valueSetVersion","valueString":"1"}),
    ]);
    invalid.push(duplicate);
    for body in invalid {
        for operation in ["$expand", "$validate-code"] {
            let mut body = body.clone();
            if operation == "$validate-code" {
                body["parameter"].as_array_mut().unwrap().extend([
                    json!({"name":"system","valueUri":"urn:codes"}),
                    json!({"name":"code","valueCode":"A"}),
                ]);
            }
            let (status, outcome) =
                request(&app, "POST", &format!("/r4/ValueSet/{operation}"), body).await;
            assert_eq!(status, 400, "{operation}: {outcome}");
            assert_eq!(outcome["resourceType"], "OperationOutcome");
        }
    }
    for operation in ["$expand", "$validate-code"] {
        let (status, _) = request(
            &app,
            "POST",
            &format!("/r4/ValueSet/{}/{operation}", Uuid::new_v4()),
            base.clone(),
        )
        .await;
        assert_eq!(status, 400);
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn inline_valueset_http_expansion_membership_and_versions() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    insert_system(&pool,json!({"url":"urn:codes","version":"1","content":"complete","caseSensitive":false,"hierarchyMeaning":"is-a","concept":[
        {"code":"Root","display":"Root","concept":[{"code":"A","display":"Alpha","property":[{"code":"group","valueCode":"one"}]},{"code":"B","display":"Beta"},{"code":"C","display":"Gamma"}]}]})).await;
    insert_system(&pool,json!({"url":"urn:codes","version":"2","content":"complete","concept":[{"code":"A","display":"Alpha 2"},{"code":"Other"}]})).await;
    insert_set(&pool,Some("import"),json!({"include":[{"system":"urn:codes","version":"1","concept":[{"code":"A"},{"code":"B"}]}]})).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    let resource = json!({"resourceType":"ValueSet","compose":{
        "include":[{"system":"urn:codes","version":"1","filter":[{"property":"concept","op":"is-a","value":"Root"}]}],
        "exclude":[{"system":"urn:codes","version":"1","concept":[{"code":"B"}]}]}});
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(
            resource.clone(),
            vec![
                json!({"name":"offset","valueInteger":1}),
                json!({"name":"count","valueInteger":1}),
            ],
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert!(body.get("url").is_none());
    assert_eq!(body["compose"], resource["compose"]);
    assert_eq!(body["expansion"]["total"], 3);
    assert_eq!(body["expansion"]["offset"], 1);
    assert_eq!(body["expansion"]["contains"].as_array().unwrap().len(), 1);
    assert_eq!(body["expansion"]["contains"][0]["code"], "C");
    assert_eq!(body["expansion"]["contains"][0]["version"], "1");
    for (code, version, display, valid) in [
        ("a", "1", "Alpha", true),
        ("B", "1", "Beta", false),
        ("Other", "1", "Other", false),
        ("A", "2", "Alpha 2", false),
        ("A", "1", "wrong", false),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            inline_parameters(
                resource.clone(),
                vec![
                    json!({"name":"system","valueUri":"urn:codes"}),
                    json!({"name":"code","valueCode":code}),
                    json!({"name":"systemVersion","valueString":version}),
                    json!({"name":"display","valueString":display}),
                ],
            ),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
    }
    let filtered = json!({"resourceType":"ValueSet","url":"urn:ad-hoc","version":"one","compose":{"include":[{"system":"urn:codes","version":"1","filter":[{"property":"group","op":"=","value":"one"}]}]}});
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(
            filtered,
            vec![
                json!({"name":"valueSetVersion","valueString":"one"}),
                json!({"name":"filter","valueString":"ALP"}),
            ],
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["url"], "urn:ad-hoc");
    assert_eq!(body["expansion"]["total"], 1);
    assert_eq!(body["expansion"]["contains"][0]["code"], "A");
    let imported = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:set|import"],"system":"urn:codes","version":"1"}],"exclude":[{"system":"urn:codes","version":"1","concept":[{"code":"B"}]}]}});
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(imported, vec![]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 1);
    assert_eq!(body["expansion"]["contains"][0]["code"], "A");
    // The systemVersion context changes both the plan key and unversioned dependency selection.
    let context = json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:codes"}]}});
    for (version, code, valid) in [
        ("1", "Root", true),
        ("2", "Other", true),
        ("2", "Root", false),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            inline_parameters(
                context.clone(),
                vec![
                    json!({"name":"system","valueUri":"urn:codes"}),
                    json!({"name":"code","valueCode":code}),
                    json!({"name":"systemVersion","valueString":version}),
                ],
            ),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
    }
    assert_eq!(
        store.count_value_sets().await.unwrap(),
        1,
        "Inline requests must not persist resources"
    );
    for (count, offset) in [(0, 0), (1, 100), (10001, 0), (1, -1)] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            inline_parameters(
                resource.clone(),
                vec![
                    json!({"name":"count","valueInteger":count}),
                    json!({"name":"offset","valueInteger":offset}),
                ],
            ),
        )
        .await;
        assert_eq!(
            status,
            if count > 10000 || offset < 0 {
                400
            } else {
                200
            },
            "{body}"
        );
        if status == 200 {
            assert_eq!(body["expansion"]["contains"].as_array().unwrap().len(), 0);
            assert_eq!(body["expansion"]["total"], 3);
        }
    }
    drop(app);
    drop(store);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn inline_valueset_cache_identity_freshness_and_eviction() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let system = insert_system(&pool,json!({"url":"urn:codes","version":"1","content":"complete","concept":[{"code":"A","display":"Alpha"},{"code":"B","display":"Beta"}]})).await;
    let set = insert_set(
        &pool,
        Some("1"),
        json!({"include":[{"system":"urn:codes","concept":[{"code":"A"}]}]}),
    )
    .await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let named = json!({"resourceType":"ValueSet","url":"urn:set","version":"1","name":"Request-local","compose":{"include":[{"system":"urn:codes","concept":[{"code":"B"}]}]}});
    let stored = store.expand_compose("urn:set", Some("1")).await.unwrap();
    let first = store.compose_inline(&named, None, None).await.unwrap();
    assert!(stored.members.find("urn:codes", None, "A").is_some());
    assert!(first.members.find("urn:codes", None, "A").is_none());
    assert!(first.members.find("urn:codes", None, "B").is_some());
    let repeated = store.compose_inline(&named, Some("1"), None).await.unwrap();
    assert!(Arc::ptr_eq(&first.members, &repeated.members));
    let mut changed = named.clone();
    changed["compose"]["include"][0]["concept"] = json!([{"code":"A"}]);
    let other = store.compose_inline(&changed, None, None).await.unwrap();
    assert!(!Arc::ptr_eq(&first.members, &other.members));
    assert!(other.members.find("urn:codes", None, "A").is_some());
    assert_eq!(
        store
            .expand_compose("urn:set", Some("1"))
            .await
            .unwrap()
            .resource["id"],
        set.to_string()
    );
    let anonymous =
        json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:codes"}]}});
    let mut workers = Vec::new();
    for _ in 0..8 {
        let store = store.clone();
        let input = anonymous.clone();
        workers.push(tokio::spawn(async move {
            store.compose_inline(&input, None, None).await.unwrap()
        }));
    }
    let shared = workers.remove(0).await.unwrap();
    for worker in workers {
        assert!(Arc::ptr_eq(&shared.members, &worker.await.unwrap().members));
    }
    // A separate database connection commits a dependency update.
    let mut writer = admin.begin().await.unwrap();
    sqlx::query(&format!("SET LOCAL search_path TO {schema}, public"))
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("UPDATE code_systems SET content=jsonb_set(content,'{concept,0,display}','\"Changed\"') WHERE id=$1").bind(system).execute(&mut *writer).await.unwrap();
    writer.commit().await.unwrap();
    let fresh = store.compose_inline(&anonymous, None, None).await.unwrap();
    assert_eq!(
        fresh
            .members
            .find("urn:codes", None, "A")
            .unwrap()
            .display
            .as_deref(),
        Some("Changed")
    );
    assert_eq!(
        shared
            .members
            .find("urn:codes", None, "A")
            .unwrap()
            .display
            .as_deref(),
        Some("Alpha")
    );
    let mut writer = admin.begin().await.unwrap();
    sqlx::query(&format!("SET LOCAL search_path TO {schema}, public"))
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("UPDATE code_systems SET content=jsonb_set(content,'{concept,0,display}','\"Rolled back\"') WHERE id=$1").bind(system).execute(&mut *writer).await.unwrap();
    writer.rollback().await.unwrap();
    assert!(Arc::ptr_eq(
        &fresh.members,
        &store
            .compose_inline(&anonymous, None, None)
            .await
            .unwrap()
            .members
    ));
    let imported =
        json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:set|1"]}]}});
    let old_import = store.compose_inline(&imported, None, None).await.unwrap();
    sqlx::query("UPDATE value_sets SET content=jsonb_set(content,'{compose}', '{\"include\":[{\"system\":\"urn:codes\",\"concept\":[{\"code\":\"B\"}]}]}') WHERE id=$1").bind(set).execute(&pool).await.unwrap();
    let new_import = store.compose_inline(&imported, None, None).await.unwrap();
    assert!(old_import.members.find("urn:codes", None, "A").is_some());
    assert!(new_import.members.find("urn:codes", None, "A").is_none());
    assert!(new_import.members.find("urn:codes", None, "B").is_some());
    for i in 0..12 {
        let mut input = named.clone();
        input["name"] = format!("Request-local-{i}").into();
        let plan = store.compose_inline(&input, None, None).await.unwrap();
        assert_eq!(plan.resource["name"], format!("Request-local-{i}"));
    }
    let retained = store.compose_inline(&named, None, None).await.unwrap();
    for i in 20..32 {
        let mut input = named.clone();
        input["name"] = i.to_string().into();
        store.compose_inline(&input, None, None).await.unwrap();
    }
    assert!(!Arc::ptr_eq(
        &retained.members,
        &store
            .compose_inline(&named, None, None)
            .await
            .unwrap()
            .members
    ));
    assert_eq!(store.count_value_sets().await.unwrap(), 1);
    sqlx::query("TRUNCATE value_sets CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.compose_inline(&imported, None, None).await,
        Err(error::AppError::NotFound(_))
    ));
    let before_failure = store.compose_inline(&anonymous, None, None).await.unwrap();
    assert_eq!(before_failure.members.values().count(), 2);
    sqlx::query(
        "ALTER TABLE terminology_generation RENAME COLUMN generation TO unavailable_generation",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        store.compose_inline(&anonymous, None, None).await.is_err(),
        "Database failure must not return a stale inline plan"
    );
    drop(store);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn inline_valueset_limits_cycles_and_unsupported_content() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    insert_system(
        &pool,
        json!({"url":"urn:codes","content":"complete","concept":[{"code":"A"}]}),
    )
    .await;
    sqlx::query("INSERT INTO code_systems(url,status,content) VALUES ('urn:stub','active','{\"resourceType\":\"CodeSystem\",\"url\":\"urn:stub\",\"content\":\"not-present\"}')").execute(&pool).await.unwrap();
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    for (resource, expected) in [
        (
            json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:missing"}]}}),
            404,
        ),
        (
            json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":["urn:missing"]}]}}),
            404,
        ),
        (
            json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:stub"}]}}),
            422,
        ),
        (
            json!({"resourceType":"ValueSet","compose":{"include":[]}}),
            422,
        ),
        (
            json!({"resourceType":"ValueSet","compose":{"include":"invalid"}}),
            400,
        ),
        (
            json!({"resourceType":"ValueSet","url":"urn:self","compose":{"include":[{"valueSet":["urn:self"]}]}}),
            422,
        ),
        (
            json!({"resourceType":"ValueSet","url":"urn:self","version":"1","compose":{"include":[{"valueSet":["urn:self|1"]}]}}),
            422,
        ),
        (
            json!({"resourceType":"ValueSet","compose":{"include":[{"system":"urn:codes","filter":[{"property":"concept","op":"generalizes","value":"A"}]}]}}),
            422,
        ),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$expand",
            inline_parameters(resource, vec![]),
        )
        .await;
        assert_eq!(status, expected, "{body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    let mut boundary = json!({"resourceType":"ValueSet","description":"","compose":{"include":[{"system":"urn:codes"}]}});
    let overhead = serde_json::to_vec(&boundary).unwrap().len();
    boundary["description"] = "x".repeat(256 * 1024 - overhead).into();
    assert_eq!(serde_json::to_vec(&boundary).unwrap().len(), 256 * 1024);
    assert_eq!(
        store
            .compose_inline(&boundary, None, None)
            .await
            .unwrap()
            .members
            .values()
            .count(),
        1
    );
    boundary["description"] = format!("{}x", boundary["description"].as_str().unwrap()).into();
    assert!(matches!(
        store.compose_inline(&boundary, None, None).await,
        Err(error::AppError::BadRequest(_))
    ));
    sqlx::query("INSERT INTO value_sets(url,status,content) SELECT 'urn:dependency:'||n,'active',jsonb_build_object('resourceType','ValueSet','compose',jsonb_build_object('include',jsonb_build_array(jsonb_build_object('system','urn:codes')))) FROM generate_series(1,128) n").execute(&pool).await.unwrap();
    let many = json!({"resourceType":"ValueSet","compose":{"include":[{"valueSet":(1..=128).map(|i|format!("urn:dependency:{i}")).collect::<Vec<_>>()}]}});
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(many, vec![]),
    )
    .await;
    assert_eq!(status, 400, "{body}");
    assert!(body["issue"][0]["diagnostics"]
        .as_str()
        .unwrap()
        .contains("resource limit"));
    drop(app);
    drop(store);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
async fn builtin_ucum_canonical_http_uses_library_without_database() {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgresql://unused:unused@127.0.0.1/unused")
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    for (code, encoded) in [
        ("mg", "mg"),
        ("mg/dL", "mg%2FdL"),
        ("mm[Hg]", "mm%5BHg%5D"),
        ("Cel", "Cel"),
    ] {
        let (status,body)=request(&app,"GET",&format!("/r4/CodeSystem/$lookup?system=http://unitsofmeasure.org&version=2.2&code={encoded}"),Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "version")["valueString"], "2.2");
        assert_eq!(parameter(&body, "name")["valueString"], "UCUM");
        assert!(!parameter(&body, "display")["valueString"]
            .as_str()
            .unwrap()
            .is_empty());
        let input = json!({"resourceType":"Parameters","parameter":[{"name":"system","valueUri":"http://unitsofmeasure.org"},{"name":"code","valueCode":code}]});
        let (status, post) = request(&app, "POST", "/r4/CodeSystem/$lookup", input.clone()).await;
        assert_eq!(status, 200, "{post}");
        assert_eq!(parameter(&post, "display"), parameter(&body, "display"));
        let (status, valid) = request(&app, "POST", "/r4/CodeSystem/$validate-code", input).await;
        assert_eq!(status, 200, "{valid}");
        assert_eq!(parameter(&valid, "result")["valueBoolean"], true);
    }
    for (code, valid) in [("mg", true), ("PAL", false), ("not-a-unit", false)] {
        let (status, body) = request(
            &app,
            "GET",
            &format!(
                "/r4/CodeSystem/$validate-code?system=http://unitsofmeasure.org%7C2.2&code={code}"
            ),
            Value::Null,
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
    }
    for (display, valid) in [("milligram", true), ("wrong", false)] {
        let (status,body)=request(&app,"GET",&format!("/r4/CodeSystem/$validate-code?system=http://unitsofmeasure.org&code=mg&display={display}"),Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
    }
    for (operation, code, version, expected) in [("$lookup", "not-a-unit", "2.2", 404)] {
        let (status,body)=request(&app,"GET",&format!("/r4/CodeSystem/{operation}?system=http://unitsofmeasure.org&code={code}&version={version}"),Value::Null).await;
        assert_eq!(status, expected, "{body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$validate-code?system=http://unitsofmeasure.org%7C2.2&version=2.1&code=mg",
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    // Dynamic UCUM inventory assertions run with the real metadata snapshot in api_completion_tests.
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn builtin_ucum_preserves_explicit_stored_versions_and_instances() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id=insert_system(&pool,json!({"url":"http://unitsofmeasure.org","version":"test-release","content":"complete","concept":[{"code":"fixture-code","display":"Stored release context"}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for path in ["/r4/CodeSystem/$lookup?system=http://unitsofmeasure.org&version=test-release&code=fixture-code".to_string(), format!("/r4/CodeSystem/{id}/$lookup?code=fixture-code")] {
        let(status,body)=request(&app,"GET",&path,Value::Null).await;
        assert_eq!(status,200,"{body}");assert_eq!(parameter(&body,"display")["valueString"],"Stored release context");assert_eq!(parameter(&body,"version")["valueString"],"test-release");
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$validate-code?system=http://unitsofmeasure.org&code=fixture-code",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    assert_eq!(parameter(&body, "version")["valueString"], "2.2");
    for operation in ["$lookup", "$validate-code"] {
        let(status,body)=request(&app,"GET",&format!("/r4/CodeSystem/{operation}?system=http://unitsofmeasure.org&version=missing-release&code=mg"),Value::Null).await;
        assert_eq!(status, 404, "{body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    drop(app);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn native_test_system(pool: &PgPool, version: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,'http://snomed.info/sct',$2,'active','RF2 fixture',$3,'snomed-rf2')")
        .bind(id).bind(version).bind(sqlx::types::Json(json!({"resourceType":"CodeSystem","url":"http://snomed.info/sct","version":version,"content":"not-present","caseSensitive":true,"hierarchyMeaning":"is-a"}))).execute(pool).await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_snomed_projection_knowledge_versions_and_fresh_hierarchy() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = native_test_system(&pool, "fixture-one").await;
    let other = native_test_system(&pool, "fixture-two").await;
    for system in [id, other] {
        sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Fixture '||n,'[]'::jsonb FROM generate_series(1,5)n").bind(system).execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO snomed_relationships(code_system_id,id,effective_time,active,module_id,source_code,destination_code,relationship_group,type_id,characteristic_type_id,modifier_id) VALUES ($1,'11','2026-02-01',true,'1','2','1',0,'116680003','900000000000011006','900000000000451002'),($1,'12','2026-02-01',true,'1','3','2',0,'116680003','900000000000011006','900000000000451002'),($1,'13','2026-02-01',false,'1','4','1',0,'116680003','900000000000011006','900000000000451002'),($1,'14','2026-02-01',true,'1','4','1',0,'999','900000000000011006','900000000000451002')").bind(id).execute(&pool).await.unwrap();
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    for (a, b, expected) in [
        ("1", "3", "subsumes"),
        ("3", "1", "subsumed-by"),
        ("2", "2", "equivalent"),
        ("1", "4", "not-subsumed"),
    ] {
        for context in [
            "$subsumes?system=http://snomed.info/sct&version=fixture-one".to_string(),
            format!("{id}/$subsumes?"),
        ] {
            let (status, body) = request(
                &app,
                "GET",
                &format!("/r4/CodeSystem/{context}&codeA={a}&codeB={b}"),
                Value::Null,
            )
            .await;
            assert_eq!(status, 200, "{body}");
            assert_eq!(parameter(&body, "outcome")["valueCode"], expected);
        }
    }
    let(status,body)=request(&app,"GET","/r4/CodeSystem/$subsumes?system=http://snomed.info/sct&version=fixture-two&codeA=1&codeB=3",Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "outcome")["valueCode"], "not-subsumed");
    for (code, expected) in [("1", true), ("missing", false)] {
        let(status,body)=request(&app,"GET",&format!("/r4/CodeSystem/$validate-code?system=http://snomed.info/sct&version=fixture-one&code={code}"),Value::Null).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], expected);
    }
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{id}/$lookup?code=1"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "display")["valueString"], "Fixture 1");
    for operation in ["$lookup", "$validate-code"] {
        let (status, body) = request(
            &app,
            "GET",
            &format!("/r4/CodeSystem/{id}/{operation}?code=100001%3A100002%3D100003"),
            Value::Null,
        )
        .await;
        assert_eq!(
            status, 422,
            "Unsupported expressions must not become invalid code answers: {body}"
        );
    }
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{id}/$subsumes?codeA=100001%2B100002&codeB=100003"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    insert_system(
        &pool,
        json!({"url":"urn:literal-code","content":"complete","concept":[{"code":"A:B"}]}),
    )
    .await;
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$validate-code?system=urn:literal-code&code=A%3AB",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    // A warm SQL-admission marker must not hide committed native edge changes.
    sqlx::query(
        "UPDATE snomed_relationships SET destination_code='4' WHERE code_system_id=$1 AND id='12'",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&id, "1", "3")
        .await
        .unwrap()
        .unwrap()
        .relationship
        .is_none());
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE snomed_relationships SET destination_code='2' WHERE code_system_id=$1 AND id='12'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    assert!(store
        .resolve_subsumption_by_id(&id, "1", "3")
        .await
        .unwrap()
        .unwrap()
        .relationship
        .is_none());
    let missing = store
        .resolve_subsumption_by_id(&id, "1", "absent")
        .await
        .unwrap()
        .unwrap();
    assert!(missing.code_b.is_none());
    let resource = json!({"resourceType":"ValueSet","compose":{"include":[{"system":"http://snomed.info/sct","version":"fixture-one"}]}});
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$expand",
        inline_parameters(resource, vec![]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["expansion"]["total"], 5);
    assert_eq!(body["expansion"]["contains"][0]["code"], "1");
    assert_eq!(body["expansion"]["contains"][0]["version"], "fixture-one");
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&id, "1", "3")
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM snomed_relationships WHERE code_system_id=$1"
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    drop(app);
    drop(store);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn native_snomed_ancestor_limit_never_returns_a_partial_answer() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = native_test_system(&pool, "fixture-limit").await;
    sqlx::query("INSERT INTO concepts(code_system_id,code) SELECT $1,n::text FROM generate_series(1,10000)n").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships(code_system_id,id,effective_time,active,module_id,source_code,destination_code,relationship_group,type_id,characteristic_type_id,modifier_id) SELECT $1,n::text,'2026-02-01',true,'1','10000',n::text,0,'116680003','900000000000011006','900000000000451002' FROM generate_series(1,9000)n").bind(id).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = request(
        &app,
        "GET",
        &format!("/r4/CodeSystem/{id}/$subsumes?codeA=1&codeB=10000"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 422, "{body}");
    assert!(body["issue"][0]["diagnostics"]
        .as_str()
        .unwrap()
        .contains("8192"));
    drop(app);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[path = "native_valueset_tests.rs"]
mod native_valueset_tests;

#[path = "implicit_membership_tests.rs"]
mod implicit_membership_tests;

#[path = "loinc_provider_tests.rs"]
mod loinc_provider_tests;

#[path = "lookup_property_tests.rs"]
mod lookup_property_tests;

#[path = "coded_input_tests.rs"]
mod coded_input_tests;

#[path = "api_completion_tests.rs"]
mod api_completion_tests;
