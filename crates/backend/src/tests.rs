//! Opt-in integration checks against a disposable schema in a local PostgreSQL database.
use super::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool};
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
    let pool = PgPoolOptions::new()
        .max_connections(3)
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
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}
