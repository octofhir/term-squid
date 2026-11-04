//! R4 single-Coding contracts exercised through the public router.
use super::{hierarchy_test_database, insert_set, insert_system, parameter, request};
use crate::{api, store::PostgresStore};
use axum::{body::Body, http::Request, Router};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

fn params(values: Vec<Value>) -> Value {
    json!({"resourceType":"Parameters","parameter":values})
}
fn coding(system: &str, version: &str, code: &str, display: Option<&str>) -> Value {
    let mut value = json!({"system":system,"version":version,"code":code,"userSelected":true});
    if let Some(display) = display {
        value["display"] = display.into();
    }
    json!({"name":"coding","valueCoding":value})
}
async fn cleanup(admin: PgPool, pool: PgPool, schema: String, app: Router) {
    drop(app);
    pool.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

#[tokio::test]
async fn malformed_and_unsupported_coded_inputs_fail_before_storage() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let valid = coding("urn:codes", "1", "A", None);
    let mut mixed = valid.clone();
    mixed["valueString"] = "A".into();
    let mut resource = valid.clone();
    resource["resource"] = json!({"resourceType":"CodeSystem"});
    let mut part = valid.clone();
    part["part"] = json!([]);
    for values in [
        vec![valid.clone(), valid.clone()],
        vec![valid.clone(), json!({"name":"code","valueCode":"A"})],
        vec![
            valid.clone(),
            json!({"name":"system","valueUri":"urn:codes"}),
        ],
        vec![
            valid.clone(),
            json!({"name":"display","valueString":"Alpha"}),
        ],
        vec![json!({"name":"coding","valueString":"A"})],
        vec![json!({"name":"coding","valueCoding":{"code":1}})],
        vec![json!({"name":"coding","valueCoding":{"system":"urn:codes"}})],
        vec![json!({"name":"coding","valueCoding":{"system":"urn:codes","code":""}})],
        vec![json!({"name":"coding","valueCoding":{"system":"urn:codes","code":"A","version":""}})],
        vec![mixed],
        vec![resource],
        vec![part],
        vec![
            valid.clone(),
            json!({"name":"displayLanguage","valueCode":"de_invalid"}),
        ],
        vec![
            valid.clone(),
            json!({"name":"date","valueDateTime":"2020-13-01"}),
        ],
        vec![json!({"name":"codeableConcept","valueCodeableConcept":{"coding":[]}})],
        vec![
            json!({"name":"coding","valueCoding":{"system":"urn:codes","code":"A","extension":[{"url":"urn:unknown"}]}}),
        ],
        vec![
            json!({"name":"system","valueUri":"urn:codes"}),
            json!({"name":"code","valueInteger":1}),
        ],
        (0..65)
            .map(|_| json!({"name":"code","valueCode":"A"}))
            .collect(),
    ] {
        for path in [
            "/r4/CodeSystem/$lookup",
            "/r4/CodeSystem/$validate-code",
            "/r4/ValueSet/$validate-code",
        ] {
            let mut fields = values.clone();
            if path.contains("validate-code") {
                fields.push(json!({"name":"url","valueUri":if path.contains("ValueSet") {"urn:set"}else{"urn:codes"}}));
            }
            let (status, body) = request(&app, "POST", path, params(fields)).await;
            assert_eq!(status, 400, "{path}: {body}");
            assert_eq!(body["resourceType"], "OperationOutcome");
        }
    }
    for path in [
        "/r4/CodeSystem/$lookup?system=urn:codes&code=A&displayLanguage=de_invalid",
        "/r4/CodeSystem/$lookup?system=urn:codes&code=A&code=B",
        "/r4/CodeSystem/$validate-code?url=urn:codes&code=A&abstract=invalid",
        "/r4/ValueSet/$validate-code?url=urn:set&system=urn:codes&code=A&version=1",
        "/r4/CodeSystem/not-a-uuid/$lookup?code=A",
        "/r4/ValueSet/not-a-uuid/$validate-code?system=urn:codes&code=A",
    ] {
        let (status, body) = request(&app, "GET", path, Value::Null).await;
        assert_eq!(status, 400, "{path}: {body}");
        assert_eq!(body["resourceType"], "OperationOutcome");
    }
    for path in [
        "/r4/CodeSystem/$lookup",
        "/r4/CodeSystem/$validate-code",
        "/r4/ValueSet/$validate-code",
    ] {
        let mut wrong = params(vec![valid.clone()]);
        wrong["resourceType"] = "ValueSet".into();
        let (status, body) = request(&app, "POST", path, wrong).await;
        assert_eq!(status, 400, "{body}");
    }
}

#[tokio::test]
async fn extraction_failures_return_fhir_operation_outcomes() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    for path in [
        "/r4/CodeSystem/$lookup",
        "/r4/CodeSystem/$validate-code",
        "/r4/ValueSet/$validate-code",
    ] {
        for (media,raw,status) in [
            ("application/fhir+json","{".to_owned(),400),
            ("application/json","[]".to_owned(),400),
            ("text/plain","{}".to_owned(),415),
            ("application/json"," ".repeat(2*1024*1024+1),413),
            ("application/json",params(vec![json!({"name":"coding","valueCoding":{"system":"urn:codes","code":"A","userSelected":"true"}})]).to_string(),400),
        ] {
            let response=app.clone().oneshot(Request::builder().method("POST").uri(path).header("content-type",media).body(Body::from(raw)).unwrap()).await.unwrap();
            assert_eq!(response.status().as_u16(),status,"{path}");
            assert_eq!(response.headers()["content-type"],"application/fhir+json");
            let body:Value=serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();assert_eq!(body["resourceType"],"OperationOutcome");
        }
    }
}

#[tokio::test]
async fn coding_uses_the_builtin_ucum_provider_without_auth_or_database_queries() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let (status, body) = request(
        &app,
        "POST",
        "/r4/CodeSystem/$lookup",
        params(vec![coding("http://unitsofmeasure.org", "2.2", "mg", None)]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "display")["valueString"], "milligram");
    let (status, body) = request(
        &app,
        "POST",
        "/r4/CodeSystem/$validate-code",
        params(vec![
            json!({"name":"url","valueUri":"http://unitsofmeasure.org"}),
            coding("http://unitsofmeasure.org", "2.2", "mg", Some("milligram")),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn coding_preserves_versions_displays_and_stored_inline_instance_contexts() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let one=insert_system(&pool,json!({"url":"urn:codes","version":"1","content":"complete","caseSensitive":false,"concept":[{"code":"A","display":"Alpha 1"}]})).await;
    insert_system(&pool,json!({"url":"urn:codes","version":"2","content":"complete","concept":[{"code":"A","display":"Alpha 2"}]})).await;
    let set = insert_set(
        &pool,
        Some("set-v1"),
        json!({"include":[{"system":"urn:codes","version":"1"}]}),
    )
    .await;
    insert_set(
        &pool,
        Some("set-v2"),
        json!({"include":[{"system":"urn:codes"}]}),
    )
    .await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for _ in 0..2 {
        for (path, extra, expected) in [
            ("/r4/CodeSystem/$lookup".to_owned(), vec![], "lookup"),
            (format!("/r4/CodeSystem/{one}/$lookup"), vec![], "lookup"),
            (
                "/r4/CodeSystem/$validate-code".to_owned(),
                vec![json!({"name":"url","valueUri":"urn:codes"})],
                "validate",
            ),
            (
                format!("/r4/CodeSystem/{one}/$validate-code"),
                vec![],
                "validate",
            ),
            (
                "/r4/ValueSet/$validate-code".to_owned(),
                vec![
                    json!({"name":"url","valueUri":"urn:set"}),
                    json!({"name":"valueSetVersion","valueString":"set-v2"}),
                ],
                "validate",
            ),
            (
                format!("/r4/ValueSet/{set}/$validate-code"),
                vec![],
                "validate",
            ),
            (
                "/r4/ValueSet/$validate-code".to_owned(),
                vec![
                    json!({"name":"valueSet","resource":{"resourceType":"ValueSet","compose":{"include":[{"system":"urn:codes"}]}}}),
                ],
                "validate",
            ),
        ] {
            let mut values = vec![coding("urn:codes", "1", "a", Some("Alpha 1"))];
            values.extend(extra);
            let (status, body) = request(&app, "POST", &path, params(values)).await;
            assert_eq!(status, 200, "{path}: {body}");
            assert_eq!(parameter(&body, "version")["valueString"], "1");
            assert_eq!(parameter(&body, "display")["valueString"], "Alpha 1");
            if expected == "validate" {
                assert_eq!(parameter(&body, "result")["valueBoolean"], true);
            }
        }
    }
    for (display, valid) in [("Alpha 1", true), ("Wrong", false)] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$validate-code",
            params(vec![
                json!({"name":"url","valueUri":"urn:codes"}),
                coding("urn:codes", "1", "A", Some(display)),
            ]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(parameter(&body, "result")["valueBoolean"], valid);
        assert_eq!(parameter(&body, "display")["valueString"], "Alpha 1");
    }
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$validate-code",
        params(vec![
            json!({"name":"url","valueUri":"urn:set"}),
            json!({"name":"valueSetVersion","valueString":"set-v1"}),
            coding("urn:codes", "2", "A", None),
        ]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], false);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM value_sets")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn coding_conflicts_cannot_override_instance_identity_in_cache_or_sql_fallback() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let one=insert_system(&pool,json!({"url":"urn:codes","version":"1","content":"complete","concept":[{"code":"A","display":"Alpha"}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for fallback in [false, true] {
        if fallback {
            sqlx::query("UPDATE concepts SET properties=$2 WHERE code_system_id=$1")
                .bind(one)
                .bind(sqlx::types::Json(
                    json!([{"code":"large","valueString":"x".repeat(65537)}]),
                ))
                .execute(&pool)
                .await
                .unwrap();
        }
        for _ in 0..2 {
            for operation in ["$lookup", "$validate-code"] {
                for (system, version, expected) in [
                    ("urn:codes", "1", 200),
                    ("urn:other", "1", 400),
                    ("urn:codes", "2", 400),
                ] {
                    let (status, body) = request(
                        &app,
                        "POST",
                        &format!("/r4/CodeSystem/{one}/{operation}"),
                        params(vec![coding(system, version, "A", None)]),
                    )
                    .await;
                    assert_eq!(status, expected, "fallback={fallback}: {body}");
                }
                for constraint in ["system=urn:other", "version=2"] {
                    let (status, body) = request(
                        &app,
                        "GET",
                        &format!("/r4/CodeSystem/{one}/{operation}?code=A&{constraint}"),
                        Value::Null,
                    )
                    .await;
                    assert_eq!(status, 400, "{body}");
                }
            }
        }
    }
    for values in [
        vec![
            json!({"name":"url","valueUri":"urn:other"}),
            coding("urn:codes", "1", "A", None),
        ],
        vec![
            json!({"name":"url","valueUri":"urn:codes|2"}),
            coding("urn:codes", "1", "A", None),
        ],
        vec![
            json!({"name":"url","valueUri":"urn:codes"}),
            json!({"name":"version","valueString":"2"}),
            coding("urn:codes", "1", "A", None),
        ],
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/CodeSystem/$validate-code",
            params(values),
        )
        .await;
        assert_eq!(status, 400, "{body}");
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/CodeSystem/$validate-code?url=urn:other&system=urn:codes&code=A",
        Value::Null,
    )
    .await;
    assert_eq!(status, 400, "{body}");
    cleanup(admin, pool, schema, app).await;
}

#[tokio::test]
async fn metadata_lists_real_operation_subsets_without_claiming_r5_support() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    let (status, body) = request(&app, "GET", "/r4/metadata", Value::Null).await;
    assert_eq!(status, 200);
    assert_eq!(body["fhirVersion"], "4.0.1");
    for (index, kind, names) in [
        (0, "CodeSystem", vec!["lookup", "validate-code", "subsumes"]),
        (1, "ValueSet", vec!["expand", "validate-code"]),
        (2, "ConceptMap", vec!["translate"]),
    ] {
        let resource = &body["rest"][0]["resource"][index];
        assert_eq!(resource["type"], kind);
        let operations = resource["operation"].as_array().unwrap();
        assert_eq!(operations.len(), names.len());
        for (operation, name) in operations.iter().zip(names) {
            assert_eq!(operation["name"], name);
            assert_eq!(
                operation["definition"],
                format!("http://hl7.org/fhir/OperationDefinition/{kind}-{name}")
            );
            assert!(!operation["documentation"].as_str().unwrap().is_empty());
        }
    }
    let (status, body) = request(
        &app,
        "GET",
        "/r4/OperationDefinition/CodeSystem-validate-code",
        Value::Null,
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["resourceType"], "OperationDefinition");
    assert!(body["parameter"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["name"] == "codeableConcept" && p["type"] == "CodeableConcept"));
}

#[tokio::test]
async fn json_format_and_post_query_rules_are_explicit() {
    let pool = PgPool::connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused").unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool)));
    for format in ["json", "application%2Fjson", "application%2Ffhir%2Bjson"] {
        let path = format!(
            "/r4/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg&_format={format}"
        );
        let (status, body) = request(&app, "GET", &path, Value::Null).await;
        assert_eq!(status, 200, "{body}");
        let (status, body) = request(
            &app,
            "POST",
            &format!("/r4/CodeSystem/$lookup?_format={format}"),
            params(vec![coding("http://unitsofmeasure.org", "2.2", "mg", None)]),
        )
        .await;
        assert_eq!(status, 200, "{body}");
    }
    for method in ["GET", "POST"] {
        for suffix in ["_format=xml", "_format=application%2Ffhir%2Bxml"] {
            let path = if method == "GET" {
                format!("/r4/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg&{suffix}")
            } else {
                format!("/r4/CodeSystem/$lookup?{suffix}")
            };
            let (status, body) = request(
                &app,
                method,
                &path,
                params(vec![coding("http://unitsofmeasure.org", "2.2", "mg", None)]),
            )
            .await;
            assert_eq!(status, 406, "{body}");
            assert_eq!(body["issue"][0]["code"], "not-supported");
        }
    }
    let (status, body) = request(
        &app,
        "POST",
        "/r4/CodeSystem/$lookup?code=other",
        params(vec![coding("http://unitsofmeasure.org", "2.2", "mg", None)]),
    )
    .await;
    assert_eq!(status, 400, "{body}");
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn coding_instance_context_observes_atomic_metadata_changes_and_rollback() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id=insert_system(&pool,json!({"url":"urn:before","version":"1","content":"complete","concept":[{"code":"A","display":"Before"}]})).await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let path = format!("/r4/CodeSystem/{id}/$validate-code");
    let old = params(vec![coding("urn:before", "1", "A", Some("Before"))]);
    let new = params(vec![coding("urn:after", "2", "A", Some("After"))]);
    let (status, body) = request(&app, "POST", &path, old.clone()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET url='urn:after',version='2' WHERE id=$1")
        .bind(id)
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("UPDATE concepts SET display='After' WHERE code_system_id=$1")
        .bind(id)
        .execute(&mut *writer)
        .await
        .unwrap();
    let (status, body) = request(&app, "POST", &path, old.clone()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    let (status, body) = request(&app, "POST", &path, new.clone()).await;
    assert_eq!(status, 400, "{body}");
    writer.commit().await.unwrap();
    let (status, body) = request(&app, "POST", &path, old).await;
    assert_eq!(status, 400, "{body}");
    let (status, body) = request(&app, "POST", &path, new.clone()).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    assert_eq!(parameter(&body, "display")["valueString"], "After");
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET url='urn:rolled-back' WHERE id=$1")
        .bind(id)
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query("UPDATE concepts SET display='Rolled back' WHERE code_system_id=$1")
        .bind(id)
        .execute(&mut *writer)
        .await
        .unwrap();
    writer.rollback().await.unwrap();
    let (status, body) = request(&app, "POST", &path, new).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(parameter(&body, "result")["valueBoolean"], true);
    assert_eq!(parameter(&body, "display")["valueString"], "After");
    cleanup(admin, pool, schema, app).await;
}
