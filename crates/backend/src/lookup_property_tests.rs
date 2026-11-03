//! HTTP regression checks for projected FHIR property arrays and legacy maps.
use super::{hierarchy_test_database, request};
use crate::{api, store::PostgresStore};
use axum::Router;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

async fn fixture(properties: Value) -> (PgPool, PgPool, String, Uuid, Router) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content) VALUES ($1,'urn:typed-properties','1','active','TypedProperties',$2)")
        .bind(id)
        .bind(sqlx::types::Json(json!({"resourceType":"CodeSystem","url":"urn:typed-properties","version":"1","content":"complete","caseSensitive":true})))
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES ($1,'A','Typed example',$2)")
        .bind(id).bind(sqlx::types::Json(properties)).execute(&pool).await.unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    (admin, pool, schema, id, app)
}

async fn cleanup(admin: PgPool, pool: PgPool, schema: String, app: Router) {
    drop(app);
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

fn properties(body: &Value) -> Vec<&Value> {
    body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|parameter| parameter["name"] == "property")
        .collect()
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn lookup_preserves_all_r4_property_types_and_repeated_properties_on_every_route() {
    let projected = json!([
        {"code":"CLASS","valueString":"CHEM"},
        {"code":"STATUS","valueCode":"ACTIVE"},
        {"code":"CLASSTYPE","valueInteger":1},
        {"code":"inactive","valueBoolean":false},
        {"code":"threshold","valueDecimal":2.125},
        {"code":"mapping","valueCoding":{"system":"urn:target","version":"v1","code":"B","display":"Target B","userSelected":true}},
        {"code":"effective","valueDateTime":"2026-02-01T12:34:56.123456+14:00"},
        {"code":"effective","valueDateTime":"2026"},
        {"code":"effective","valueDateTime":"2026-02"},
        {"code":"effective","valueDateTime":"2024-02-29"}
    ]);
    let expected = json!([
        {"name":"property","part":[{"name":"code","valueCode":"CLASS"},{"name":"value","valueString":"CHEM"}]},
        {"name":"property","part":[{"name":"code","valueCode":"STATUS"},{"name":"value","valueCode":"ACTIVE"}]},
        {"name":"property","part":[{"name":"code","valueCode":"CLASSTYPE"},{"name":"value","valueInteger":1}]},
        {"name":"property","part":[{"name":"code","valueCode":"inactive"},{"name":"value","valueBoolean":false}]},
        {"name":"property","part":[{"name":"code","valueCode":"threshold"},{"name":"value","valueDecimal":2.125}]},
        {"name":"property","part":[{"name":"code","valueCode":"mapping"},{"name":"value","valueCoding":{"system":"urn:target","version":"v1","code":"B","display":"Target B","userSelected":true}}]},
        {"name":"property","part":[{"name":"code","valueCode":"effective"},{"name":"value","valueDateTime":"2026-02-01T12:34:56.123456+14:00"}]},
        {"name":"property","part":[{"name":"code","valueCode":"effective"},{"name":"value","valueDateTime":"2026"}]},
        {"name":"property","part":[{"name":"code","valueCode":"effective"},{"name":"value","valueDateTime":"2026-02"}]},
        {"name":"property","part":[{"name":"code","valueCode":"effective"},{"name":"value","valueDateTime":"2024-02-29"}]}
    ]);
    let (admin, pool, schema, id, app) = fixture(projected).await;
    for (method, path, input) in [
        (
            "GET",
            "/r4/CodeSystem/$lookup?system=urn:typed-properties&version=1&code=A".to_owned(),
            Value::Null,
        ),
        (
            "POST",
            "/r4/CodeSystem/$lookup".to_owned(),
            json!({"resourceType":"Parameters","parameter":[{"name":"system","valueUri":"urn:typed-properties"},{"name":"version","valueString":"1"},{"name":"code","valueCode":"A"}]}),
        ),
        (
            "GET",
            format!("/r4/CodeSystem/{id}/$lookup?code=A"),
            Value::Null,
        ),
        (
            "POST",
            format!("/r4/CodeSystem/{id}/$lookup"),
            json!({"resourceType":"Parameters","parameter":[{"name":"code","valueCode":"A"}]}),
        ),
    ] {
        let (status, body) = request(&app, method, &path, input).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["resourceType"], "Parameters");
        assert_eq!(json!(properties(&body)), expected, "{method} {path}");
        assert!(body["parameter"]
            .as_array()
            .unwrap()
            .contains(&json!({"name":"version","valueString":"1"})));
    }
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn lookup_property_updates_switch_types_without_stale_or_coerced_values() {
    let (admin, pool, schema, id, app) = fixture(json!([{"code":"rank","valueInteger":1}])).await;
    let path = format!("/r4/CodeSystem/{id}/$lookup?code=A");
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        properties(&body)[0]["part"][1],
        json!({"name":"value","valueInteger":1})
    );

    let mut transaction = pool.begin().await.unwrap();
    sqlx::query("UPDATE concepts SET properties=$2 WHERE code_system_id=$1")
        .bind(id)
        .bind(sqlx::types::Json(
            json!([{"code":"rank","valueString":"rolled back"}]),
        ))
        .execute(&mut *transaction)
        .await
        .unwrap();
    transaction.rollback().await.unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        properties(&body)[0]["part"][1],
        json!({"name":"value","valueInteger":1})
    );

    sqlx::query("UPDATE concepts SET properties=$2 WHERE code_system_id=$1")
        .bind(id)
        .bind(sqlx::types::Json(
            json!([{"code":"rank","valueString":"one"}]),
        ))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        properties(&body)[0]["part"][1],
        json!({"name":"value","valueString":"one"})
    );

    // Legacy map projections keep their pre-existing string-valued representation.
    sqlx::query("UPDATE concepts SET properties=$2 WHERE code_system_id=$1")
        .bind(id)
        .bind(sqlx::types::Json(
            json!({"rank":7,"flag":true,"label":"old"}),
        ))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        json!(properties(&body)),
        json!([
            {"name":"property","part":[{"name":"code","valueCode":"flag"},{"name":"value","valueString":"true"}]},
            {"name":"property","part":[{"name":"code","valueCode":"label"},{"name":"value","valueString":"\"old\""}]},
            {"name":"property","part":[{"name":"code","valueCode":"rank"},{"name":"value","valueString":"7"}]}
        ])
    );
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn lookup_never_silently_drops_malformed_or_unsupported_property_payloads() {
    let (admin, pool, schema, id, app) = fixture(json!([])).await;
    let path = format!("/r4/CodeSystem/{id}/$lookup?code=A");
    for payload in [
        json!(null),
        json!("wrong root"),
        json!([null]),
        json!([{}]),
        json!([{"code":"","valueString":"x"}]),
        json!([{"code":" rank","valueString":"x"}]),
        json!([{"code":"rank","valueString":"x","valueInteger":1}]),
        json!([{"code":"rank","valueUri":"urn:x"}]),
        json!([{"code":"rank","valueString":""}]),
        json!([{"code":"rank","valueInteger":"1"}]),
        json!([{"code":"rank","valueInteger":2147483648_i64}]),
        json!([{"code":"rank","valueInteger":1.5}]),
        json!([{"code":"rank","valueBoolean":"true"}]),
        json!([{"code":"rank","valueDecimal":"1.25"}]),
        json!([{"code":"rank","valueDateTime":"2026-02-30"}]),
        json!([{"code":"rank","valueDateTime":"2026-13"}]),
        json!([{"code":"rank","valueDateTime":"0000"}]),
        json!([{"code":"rank","valueDateTime":"2026-02-01T12:00:00"}]),
        json!([{"code":"rank","valueDateTime":"2026-02-01T12:00:00+14:01"}]),
        json!([{"code":"rank","valueDateTime":"2026-02-01T12:00:00z"}]),
        json!([{"code":"rank","valueCoding":{}}]),
        json!([{"code":"rank","valueCoding":{"code":7}}]),
        json!([{"code":"rank","valueCoding":{"code":"x","userSelected":"true"}}]),
        json!([{"code":"rank","valueCoding":{"code":"x","extension":[]}}]),
        json!([{"code":"rank","valueString":"x","_valueString":{"extension":[]}}]),
        json!([{"code":"rank","valueString":"x","modifierExtension":[]}]),
    ] {
        sqlx::query("UPDATE concepts SET properties=$2 WHERE code_system_id=$1")
            .bind(id)
            .bind(sqlx::types::Json(&payload))
            .execute(&pool)
            .await
            .unwrap();
        let (status, body) = request(&app, "GET", &path, Value::Null).await;
        assert_eq!(status, 422, "payload={payload}, response={body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
        assert_eq!(body["issue"][0]["code"], "not-supported");
        assert!(body["issue"][0]["diagnostics"]
            .as_str()
            .unwrap()
            .contains("stored concept property"));
        assert!(body.get("parameter").is_none(), "{body}");
    }
    sqlx::query("UPDATE concepts SET properties='[]'::jsonb WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert!(properties(&body).is_empty());
    sqlx::query("UPDATE concepts SET properties=NULL WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = request(&app, "GET", &path, Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert!(properties(&body).is_empty());
    cleanup(admin, pool, schema, app).await;
}
