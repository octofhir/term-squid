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
