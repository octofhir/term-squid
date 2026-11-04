use super::*;
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use std::sync::atomic::{AtomicBool, Ordering};
use tower::ServiceExt;

async fn call(
    app: &Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    csrf: Option<&str>,
    origin: Option<&str>,
    body: Value,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/fhir+json");
    if let Some(c) = cookie {
        builder = builder.header(header::COOKIE, c);
    }
    if let Some(c) = csrf {
        builder = builder.header("x-csrf-token", c);
    }
    if let Some(o) = origin {
        builder = builder.header(header::ORIGIN, o);
    }
    let response = app
        .clone()
        .oneshot(
            builder
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, serde_json::from_slice(&bytes).unwrap())
}
async fn database() -> (PgPool, Shared, String) {
    use argon2::{password_hash::SaltString, PasswordHasher};
    let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
    let admin = PgPool::connect(&url).await.unwrap();
    let schema = format!("test_admin_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .unwrap();
    let path = format!("SET search_path TO {schema},public");
    let pool = crate::database::pool_options(5)
        .after_connect(move |c, _| {
            let path = path.clone();
            Box::pin(async move {
                sqlx::query(&path).execute(c).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let salt = SaltString::encode_b64(b"test-only-password-salt").unwrap();
    let hash = Argon2::default()
        .hash_password(b"test-only-password-123", &salt)
        .unwrap()
        .to_string();
    let staging = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.benchmarks/ui-admin/tests")
        .join(&schema);
    tokio::fs::create_dir_all(&staging).await.unwrap();
    let state = Arc::new(AdminState {
        pool,
        config: Some(AdminConfig {
            username: "test-admin".into(),
            password_hash: hash,
            origin: "http://localhost:8082".into(),
            secure: false,
            ttl_seconds: 60,
            staging,
        }),
        uploads: Semaphore::new(2),
        password_checks: Semaphore::new(1),
        available: AtomicBool::new(true),
    });
    (admin, state, schema)
}
async fn teardown(admin: PgPool, state: Shared, schema: String) {
    let dir = state.config.as_ref().unwrap().staging.clone();
    state.pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
    tokio::fs::remove_dir_all(dir).await.unwrap();
}
async fn login_session(app: &Router) -> (String, String) {
    let (status, headers, _) = call(
        app,
        "POST",
        "/login",
        None,
        None,
        Some("http://localhost:8082"),
        json!({"username":"test-admin","password":"test-only-password-123"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cookie = headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert!(headers[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .contains("HttpOnly; SameSite=Strict; Path=/api/admin"));
    let (status, _, session) = call(
        app,
        "GET",
        "/session",
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(session["administrator"]["username"], "test-admin");
    (cookie, session["csrfToken"].as_str().unwrap().into())
}
async fn upload(app: &Router, cookie: &str, csrf: &str, content: Value) -> Uuid {
    let (status, _, job) = call(
        app,
        "POST",
        "/jobs",
        Some(cookie),
        Some(csrf),
        Some("http://localhost:8082"),
        content,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{job}");
    assert_eq!(job["state"], "ready");
    Uuid::parse_str(job["id"].as_str().unwrap()).unwrap()
}
fn resource(url: &str) -> Value {
    json!({"resourceType":"CodeSystem","url":url,"version":"1","status":"active","content":"complete","hierarchyMeaning":"is-a","concept":[{"code":"root","concept":[{"code":"child","display":"Child"}]}]})
}

#[tokio::test]
async fn unconfigured_admin_does_not_need_auth_database() {
    let pool = crate::database::pool_options(1)
        .connect_lazy("postgresql://unused:unused@127.0.0.1:1/unused")
        .unwrap();
    let app = router(pool, None).await.unwrap();
    let (status, _, data) = call(&app, "GET", "/session", None, None, None, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["configured"], false);
    for path in ["/jobs", "/login"] {
        assert_eq!(
            call(&app, "POST", path, None, None, None, Value::Null)
                .await
                .0,
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn admin_auth_precedes_body_io_and_sessions_are_revoked() {
    let (admin, state, schema) = database().await;
    let app = routes(state.clone());
    let polled = Arc::new(AtomicBool::new(false));
    let flag = polled.clone();
    let body = Body::from_stream(futures_util::stream::once(async move {
        flag.store(true, Ordering::SeqCst);
        Ok::<_, std::io::Error>("should never read")
    }));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs")
                .header(header::CONTENT_TYPE, "application/gzip")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(!polled.load(Ordering::SeqCst));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_import_jobs")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(tokio::fs::read_dir(&state.config.as_ref().unwrap().staging)
        .await
        .unwrap()
        .next_entry()
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        call(
            &app,
            "POST",
            "/login",
            None,
            None,
            Some("https://evil.example"),
            json!({})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/login",
            None,
            None,
            Some("http://localhost:8082"),
            json!({"username":"test-admin","password":"wrong"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (cookie, csrf) = login_session(&app).await;
    for (origin, token) in [
        (None, Some(csrf.as_str())),
        (Some("https://evil.example"), Some(csrf.as_str())),
        (Some("http://localhost:8082"), None),
    ] {
        assert_eq!(
            call(
                &app,
                "POST",
                "/jobs",
                Some(&cookie),
                token,
                origin,
                resource("urn:denied")
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_import_jobs")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        call(
            &app,
            "POST",
            "/logout",
            Some(&cookie),
            Some(&csrf),
            Some("http://localhost:8082"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(&app, "GET", "/jobs", Some(&cookie), None, None, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (cookie, _) = login_session(&app).await;
    sqlx::query("UPDATE admin_sessions SET expires_at=NOW()-INTERVAL '1 second'")
        .execute(&state.pool)
        .await
        .unwrap();
    assert_eq!(
        call(&app, "GET", "/jobs", Some(&cookie), None, None, Value::Null)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    sqlx::query("UPDATE admin_login_limit SET attempts=5,window_start=NOW()")
        .execute(&state.pool)
        .await
        .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            "/login",
            None,
            None,
            Some("http://localhost:8082"),
            json!({"username":"test-admin","password":"test-only-password-123"})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    teardown(admin, state, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn uploads_publish_projections_atomically_and_preserve_immutable_versions() {
    let (admin, state, schema) = database().await;
    let app = routes(state.clone());
    let (cookie, csrf) = login_session(&app).await;
    let content = resource("urn:admin-import");
    let id = upload(&app, &cookie, &csrf, content.clone()).await;
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/jobs/{id}/start"),
            Some(&cookie),
            Some(&csrf),
            Some("http://localhost:8082"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(jobs::claim(&state).await.unwrap(), Some(id));
    jobs::process(state.clone(), id).await;
    let (_, _, job) = call(
        &app,
        "GET",
        &format!("/jobs/{id}"),
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(job["result"]["unchanged"], false);
    let concepts: i64 = sqlx::query_scalar("SELECT count(*) FROM concepts")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(concepts, 2);
    let closure: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM closure_table WHERE ancestor_code='root' AND descendant_code='child'",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(closure, 1);
    let public = crate::api::create_router(Arc::new(crate::store::PostgresStore::new(
        state.pool.clone(),
    )));
    for method in ["GET", "POST"] {
        let path = if method == "GET" {
            "/r4/CodeSystem/$lookup?system=urn:admin-import&code=child"
        } else {
            "/r4/CodeSystem/$lookup"
        };
        let body = if method == "POST" {
            json!({"resourceType":"Parameters","parameter":[{"name":"system","valueUri":"urn:admin-import"},{"name":"code","valueCode":"child"}]})
        } else {
            Value::Null
        };
        let (status, _, _) = call(&public, method, path, None, None, None, body).await;
        assert_eq!(status, StatusCode::OK);
    }
    for (value, expected) in [
        (content.clone(), "succeeded"),
        (
            {
                let mut c = content.clone();
                c["concept"][0]["code"] = "changed".into();
                c
            },
            "failed",
        ),
    ] {
        let id = upload(&app, &cookie, &csrf, value).await;
        call(
            &app,
            "POST",
            &format!("/jobs/{id}/start"),
            Some(&cookie),
            Some(&csrf),
            Some("http://localhost:8082"),
            Value::Null,
        )
        .await;
        jobs::claim(&state).await.unwrap();
        jobs::process(state.clone(), id).await;
        let (_, _, job) = call(
            &app,
            "GET",
            &format!("/jobs/{id}"),
            Some(&cookie),
            None,
            None,
            Value::Null,
        )
        .await;
        assert_eq!(job["state"], expected, "{job}");
        if expected == "succeeded" {
            assert_eq!(job["result"]["unchanged"], true);
        }
    }
    assert_eq!(
        call(
            &app,
            "POST",
            "/jobs",
            Some(&cookie),
            Some(&csrf),
            Some("http://localhost:8082"),
            json!({"resourceType":"Bundle"})
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let concepts: i64 = sqlx::query_scalar("SELECT count(*) FROM concepts")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(concepts, 2);
    teardown(admin, state, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn cancelled_corrupted_and_restarted_jobs_never_publish() {
    let (admin, state, schema) = database().await;
    let app = routes(state.clone());
    let (cookie, csrf) = login_session(&app).await;
    let id = upload(&app, &cookie, &csrf, resource("urn:cancelled")).await;
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/jobs/{id}/cancel"),
            Some(&cookie),
            Some(&csrf),
            Some("http://localhost:8082"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    call(
        &app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    assert_eq!(jobs::claim(&state).await.unwrap(), None);
    let cancelled = upload(&app, &cookie, &csrf, resource("urn:cancel-at-commit")).await;
    call(
        &app,
        "POST",
        &format!("/jobs/{cancelled}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    assert_eq!(jobs::claim(&state).await.unwrap(), Some(cancelled));
    call(
        &app,
        "POST",
        &format!("/jobs/{cancelled}/cancel"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    jobs::process(state.clone(), cancelled).await;
    let (_, _, cancelled_job) = call(
        &app,
        "GET",
        &format!("/jobs/{cancelled}"),
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(cancelled_job["state"], "cancelled");
    let id = upload(&app, &cookie, &csrf, resource("urn:corrupt")).await;
    tokio::fs::write(
        state
            .config
            .as_ref()
            .unwrap()
            .staging
            .join(format!("{id}.json")),
        resource("urn:tampered").to_string(),
    )
    .await
    .unwrap();
    call(
        &app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    jobs::claim(&state).await.unwrap();
    jobs::process(state.clone(), id).await;
    let (_, _, job) = call(
        &app,
        "GET",
        &format!("/jobs/{id}"),
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(job["state"], "failed");
    let id = upload(&app, &cookie, &csrf, resource("urn:restart")).await;
    call(
        &app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    jobs::recover(&state).await.unwrap();
    let (_, _, job) = call(
        &app,
        "GET",
        &format!("/jobs/{id}"),
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(job["state"], "interrupted");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM code_systems")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    teardown(admin, state, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn executing_cancellation_and_projection_failure_roll_back_publication() {
    let (admin, state, schema) = database().await;
    let app = routes(state.clone());
    let (cookie, csrf) = login_session(&app).await;
    let id = upload(&app, &cookie, &csrf, resource("urn:running")).await;
    let mut blocker = state.pool.acquire().await.unwrap();
    let key = "code_systems|urn:running|1";
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut *blocker)
        .await
        .unwrap();
    call(
        &app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    assert_eq!(jobs::claim(&state).await.unwrap(), Some(id));
    let worker_state = state.clone();
    let worker = tokio::spawn(async move { jobs::process(worker_state, id).await });
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let status: String =
                sqlx::query_scalar("SELECT state FROM admin_import_jobs WHERE id=$1")
                    .bind(id)
                    .fetch_one(&state.pool)
                    .await
                    .unwrap();
            if status == "publishing" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("/jobs/{id}/cancel"),
            Some(&cookie),
            Some(&csrf),
            Some("http://localhost:8082"),
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    tokio::time::timeout(std::time::Duration::from_secs(3), worker)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut *blocker)
        .await
        .unwrap();
    drop(blocker);
    let (_, _, job) = call(
        &app,
        "GET",
        &format!("/jobs/{id}"),
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(job["state"], "cancelled");
    sqlx::raw_sql("CREATE FUNCTION reject_admin_projection() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected projection failure'; END $$; CREATE TRIGGER reject_admin_projection BEFORE INSERT ON closure_table FOR EACH STATEMENT EXECUTE FUNCTION reject_admin_projection();").execute(&state.pool).await.unwrap();
    let id = upload(&app, &cookie, &csrf, resource("urn:projection-failure")).await;
    call(
        &app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    jobs::claim(&state).await.unwrap();
    jobs::process(state.clone(), id).await;
    let (_, _, job) = call(
        &app,
        "GET",
        &format!("/jobs/{id}"),
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(job["state"], "failed");
    for table in ["code_systems", "concepts", "closure_table"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&state.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    teardown(admin, state, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn concurrent_unversioned_imports_are_idempotent_and_refresh_public_negatives() {
    let (admin, state, schema) = database().await;
    let mut content = resource("urn:unversioned");
    content.as_object_mut().unwrap().remove("version");
    let meta = term_squid_ingestion::inspect(&content).unwrap();
    let import = |pool: PgPool, content: Value, meta: term_squid_ingestion::Inspection| async move {
        let mut tx = pool.begin().await.unwrap();
        let result = term_squid_ingestion::import_resource(&mut tx, &content, &meta)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        result
    };
    let (a, b) = tokio::join!(
        import(state.pool.clone(), content.clone(), meta.clone()),
        import(state.pool.clone(), content, meta)
    );
    assert_eq!(a.id, b.id);
    assert_ne!(a.unchanged, b.unchanged);
    let public = crate::api::create_router(Arc::new(crate::store::PostgresStore::new(
        state.pool.clone(),
    )));
    let path = "/r4/CodeSystem/$validate-code?system=urn:unversioned&code=new";
    let (status, _, before) = call(&public, "GET", path, None, None, None, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(before["parameter"][0]["valueBoolean"], false);
    let app = routes(state.clone());
    let (cookie, csrf) = login_session(&app).await;
    let mut next = resource("urn:unversioned");
    next["version"] = "2".into();
    next["concept"] = json!([{"code":"new","display":"Newly activated"}]);
    let id = upload(&app, &cookie, &csrf, next).await;
    call(
        &app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    jobs::claim(&state).await.unwrap();
    jobs::process(state.clone(), id).await;
    let (status, _, after) = call(&public, "GET", path, None, None, None, Value::Null).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(after["parameter"][0]["valueBoolean"], true);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM code_systems WHERE url='urn:unversioned'")
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
    teardown(admin, state, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn public_catalog_pages_filter_versions_and_keep_projected_concepts() {
    let (admin, state, schema) = database().await;
    let mut projected = Uuid::nil();
    for n in 0..40 {
        let id = Uuid::new_v4();
        projected = id;
        let content = json!({"resourceType":"CodeSystem","url":"urn:catalog","version":format!("{n:03}"),"name":format!("Catalog {n}"),"status":"active","content":"not-present","description":"Projected source metadata"});
        sqlx::query("INSERT INTO code_systems(id,url,version,name,status,content) VALUES($1,'urn:catalog',$2,$3,'active',$4)")
            .bind(id).bind(format!("{n:03}")).bind(format!("Catalog {n}")).bind(sqlx::types::Json(content)).execute(&state.pool).await.unwrap();
    }
    sqlx::query("INSERT INTO concepts(code_system_id,code,display) VALUES($1,'projected','Projection-only concept')").bind(projected).execute(&state.pool).await.unwrap();
    let app = crate::catalog::router(state.pool.clone());
    let (_, _, first) = call(
        &app,
        "GET",
        "/api/catalog/CodeSystem?url=urn:catalog",
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    let (_, _, second) = call(
        &app,
        "GET",
        "/api/catalog/CodeSystem?url=urn:catalog&offset=25",
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(first["total"], 40);
    assert_eq!(first["items"].as_array().unwrap().len(), 25);
    assert_eq!(second["items"].as_array().unwrap().len(), 15);
    let ids: std::collections::HashSet<_> = first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(second["items"].as_array().unwrap())
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 40);
    for row in first["items"].as_array().unwrap() {
        assert!(row.get("concept").is_none());
        assert_eq!(row["description"], "Projected source metadata");
    }
    let (_, _, selected) = call(
        &app,
        "GET",
        "/api/catalog/CodeSystem?url=urn:catalog&version=030",
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(selected["total"], 1);
    assert_eq!(selected["items"][0]["version"], "030");
    let (_, _, search) = call(
        &app,
        "GET",
        "/api/catalog/CodeSystem?q=Catalog%203",
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(search["total"], 11);
    let (_, _, literal) = call(
        &app,
        "GET",
        "/api/catalog/CodeSystem?q=%25_",
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(literal["total"], 0);
    let (_, _, concepts) = call(
        &app,
        "GET",
        &format!("/api/catalog/CodeSystem/{projected}/concepts"),
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(concepts["total"], 1);
    assert_eq!(concepts["items"][0]["code"], "projected");
    teardown(admin, state, schema).await;
}

fn package_bytes(resources: &[Value]) -> Vec<u8> {
    use flate2::{write::GzEncoder, Compression};
    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    for (index, value) in std::iter::once(
        json!({"name":"org.example.test","version":"1.0.0","fhirVersions":["4.0.1"]}),
    )
    .chain(resources.iter().cloned())
    .enumerate()
    {
        let bytes = serde_json::to_vec(&value).unwrap();
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        let name = if index == 0 {
            "package/package.json".to_string()
        } else {
            format!("package/resource-{index}.json")
        };
        archive
            .append_data(&mut header, name, bytes.as_slice())
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}
async fn upload_package(
    app: &Router,
    cookie: &str,
    csrf: &str,
    bytes: Vec<u8>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/jobs")
                .header(header::CONTENT_TYPE, "application/gzip")
                .header(header::COOKIE, cookie)
                .header("x-csrf-token", csrf)
                .header(header::ORIGIN, "http://localhost:8082")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn run_job(app: &Router, state: &Shared, cookie: &str, csrf: &str, id: Uuid) -> Value {
    let (status, _, _) = call(
        app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(cookie),
        Some(csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(jobs::claim(state).await.unwrap(), Some(id));
    jobs::process(state.clone(), id).await;
    call(
        app,
        "GET",
        &format!("/jobs/{id}"),
        Some(cookie),
        None,
        None,
        Value::Null,
    )
    .await
    .2
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn packages_activate_atomically_and_conflicts_roll_back_all_resources() {
    let (admin, state, schema) = database().await;
    let app = routes(state.clone());
    let (cookie, csrf) = login_session(&app).await;
    let system = resource("urn:package:existing");
    let set = json!({"resourceType":"ValueSet","url":"urn:package:set","version":"1","status":"active","compose":{"include":[{"system":"urn:package:existing"}]}});
    let bytes = package_bytes(&[system.clone(), set]);
    let (status, job) = upload_package(&app, &cookie, &csrf, bytes.clone()).await;
    assert_eq!(status, StatusCode::OK, "{job}");
    assert_eq!(job["format"], "fhir-package");
    assert_eq!(job["inspection"]["resources"].as_array().unwrap().len(), 2);
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM code_systems")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(before, 0);
    let id = Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
    let job = run_job(&app, &state, &cookie, &csrf, id).await;
    assert_eq!(job["state"], "succeeded", "{job}");
    assert_eq!(job["result"]["imported"], 2);
    assert_eq!(job["result"]["unchanged"], 0);
    let public = crate::api::create_router(Arc::new(crate::store::PostgresStore::new(
        state.pool.clone(),
    )));
    let (status, _, expanded) = call(
        &public,
        "GET",
        "/r4/ValueSet/$expand?url=urn:package:set",
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{expanded}");
    assert_eq!(expanded["expansion"]["total"], 2);
    let (_, job) = upload_package(&app, &cookie, &csrf, bytes).await;
    let job = run_job(
        &app,
        &state,
        &cookie,
        &csrf,
        Uuid::parse_str(job["id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(job["result"]["unchanged"], 2);
    assert_eq!(job["result"]["imported"], 0);
    let mut conflicting = system.clone();
    conflicting["concept"][0]["code"] = "changed".into();
    let (_, job) = upload_package(
        &app,
        &cookie,
        &csrf,
        package_bytes(&[resource("urn:a-new-before-conflict"), conflicting]),
    )
    .await;
    let job = run_job(
        &app,
        &state,
        &cookie,
        &csrf,
        Uuid::parse_str(job["id"].as_str().unwrap()).unwrap(),
    )
    .await;
    assert_eq!(job["state"], "failed", "{job}");
    let urls: Vec<String> = sqlx::query_scalar("SELECT url FROM code_systems")
        .fetch_all(&state.pool)
        .await
        .unwrap();
    assert_eq!(urls, vec!["urn:package:existing"]);
    let concepts: i64 = sqlx::query_scalar("SELECT count(*) FROM concepts")
        .fetch_one(&state.pool)
        .await
        .unwrap();
    assert_eq!(concepts, 2);
    let (_, _, expanded) = call(
        &public,
        "GET",
        "/r4/ValueSet/$expand?url=urn:package:set",
        None,
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(expanded["expansion"]["total"], 2);
    assert_eq!(
        upload_package(&app, &cookie, &csrf, b"invalid gzip".to_vec())
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    teardown(admin, state, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn package_cancellation_corruption_and_restart_preserve_publication() {
    let (admin, state, schema) = database().await;
    let app = routes(state.clone());
    let (cookie, csrf) = login_session(&app).await;
    for mode in ["cancel", "checksum", "restart"] {
        let (_, job) = upload_package(
            &app,
            &cookie,
            &csrf,
            package_bytes(&[resource(&format!("urn:package:{mode}"))]),
        )
        .await;
        assert_eq!(job["state"], "ready", "{job}");
        let id = Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
        match mode {
            "cancel" => {
                call(
                    &app,
                    "POST",
                    &format!("/jobs/{id}/cancel"),
                    Some(&cookie),
                    Some(&csrf),
                    Some("http://localhost:8082"),
                    Value::Null,
                )
                .await;
            }
            "checksum" => {
                tokio::fs::write(
                    state
                        .config
                        .as_ref()
                        .unwrap()
                        .staging
                        .join(format!("{id}.json")),
                    package_bytes(&[resource("urn:tampered-package")]),
                )
                .await
                .unwrap();
                let result = run_job(&app, &state, &cookie, &csrf, id).await;
                assert_eq!(result["state"], "failed");
            }
            _ => {
                call(
                    &app,
                    "POST",
                    &format!("/jobs/{id}/start"),
                    Some(&cookie),
                    Some(&csrf),
                    Some("http://localhost:8082"),
                    Value::Null,
                )
                .await;
                jobs::recover(&state).await.unwrap();
            }
        }
        let (_, _, job) = call(
            &app,
            "GET",
            &format!("/jobs/{id}"),
            Some(&cookie),
            None,
            None,
            Value::Null,
        )
        .await;
        assert_eq!(
            job["state"],
            match mode {
                "cancel" => "cancelled",
                "checksum" => "failed",
                _ => "interrupted",
            }
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        0
    );
    teardown(admin, state, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn executing_package_cancellation_rolls_back_earlier_resources() {
    let (admin, state, schema) = database().await;
    let app = routes(state.clone());
    let (cookie, csrf) = login_session(&app).await;
    let key = "code_systems|urn:package:z-blocked|1";
    let mut blocker = state.pool.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut *blocker)
        .await
        .unwrap();
    let (_, job) = upload_package(
        &app,
        &cookie,
        &csrf,
        package_bytes(&[
            resource("urn:package:a-first"),
            resource("urn:package:z-blocked"),
        ]),
    )
    .await;
    assert_eq!(job["state"], "ready");
    let id = Uuid::parse_str(job["id"].as_str().unwrap()).unwrap();
    call(
        &app,
        "POST",
        &format!("/jobs/{id}/start"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    assert_eq!(jobs::claim(&state).await.unwrap(), Some(id));
    let worker_state = state.clone();
    let worker = tokio::spawn(async move { jobs::process(worker_state, id).await });
    // The second resource is waiting for its identity lock. The first has already
    // been inserted and projected in the same, still-uncommitted transaction.
    tokio::time::timeout(std::time::Duration::from_secs(3),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE locktype='advisory' AND NOT granted AND classid=((hashtextextended($1,0)>>32)&4294967295)::oid AND objid=(hashtextextended($1,0)&4294967295)::oid)").bind(key).fetch_one(&state.pool).await.unwrap();
            if waiting {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        0
    );
    let (status, _, _) = call(
        &app,
        "POST",
        &format!("/jobs/{id}/cancel"),
        Some(&cookie),
        Some(&csrf),
        Some("http://localhost:8082"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    tokio::time::timeout(std::time::Duration::from_secs(3), worker)
        .await
        .unwrap()
        .unwrap();
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended($1,0))")
        .bind(key)
        .execute(&mut *blocker)
        .await
        .unwrap();
    drop(blocker);
    let (_, _, job) = call(
        &app,
        "GET",
        &format!("/jobs/{id}"),
        Some(&cookie),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(job["state"], "cancelled");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM code_systems")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM concepts")
            .fetch_one(&state.pool)
            .await
            .unwrap(),
        0
    );
    teardown(admin, state, schema).await;
}
