//! Generated RF2 fixtures for the implicit membership operation boundary.
use super::*;
use crate::store::{ValueSetOperation, ValueSetRequest, ValueSetResult};
use term_squid_core::snomed::{INTERNATIONAL, SYSTEM};

const OLD: &str = "http://snomed.info/sct/900000000000207008/version/20260101";
const CURRENT: &str = "http://snomed.info/sct/900000000000207008/version/20260201";
const NEXT: &str = "http://snomed.info/sct/900000000000207008/version/20260401";
const UK_MODULE: &str = "83821000000107";
const UK: &str = "http://snomed.info/sct/83821000000107/version/20260301";

async fn release(pool: &PgPool, module: &str, date: &str, version: &str, label: &str) -> Uuid {
    let id = native_test_system(pool, version).await;
    sqlx::query("INSERT INTO snomed_releases VALUES($1,$2,$3,'900000000000509007','en',repeat('a',64),'{\"generatedFixture\":true}')")
        .bind(id).bind(module).bind(chrono::NaiveDate::parse_from_str(date,"%Y-%m-%d").unwrap()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,$2::text||' '||n,CASE WHEN n=5 THEN '[{\"code\":\"inactive\",\"valueBoolean\":true}]'::jsonb ELSE '[]'::jsonb END FROM generate_series(1,5)n UNION ALL SELECT $1,'111','Fixture attribute','[]'::jsonb")
        .bind(id).bind(label).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships VALUES($1,'2-parent','2026-02-01',true,'1','2','1',0,'116680003','900000000000011006','900000000000451002'),($1,'3-parent','2026-02-01',true,'1','3','2',0,'116680003','900000000000011006','900000000000451002'),($1,'4-stated','2026-02-01',true,'1','4','1',0,'116680003','900000000000010007','900000000000451002'),($1,'5-inactive','2026-02-01',false,'1','5','1',0,'116680003','900000000000011006','900000000000451002'),($1,'4-attribute','2026-02-01',true,'1','4','1',0,'111','900000000000011006','900000000000451002')")
        .bind(id).execute(pool).await.unwrap();
    id
}
fn whole(base: &str) -> String {
    format!("{base}?fhir_vs")
}
fn isa(base: &str, anchor: &str) -> String {
    format!("{base}?fhir_vs=isa/{anchor}")
}
fn parameters(url: &str, system: &str, code: &str, extra: Vec<Value>) -> Value {
    let mut values = vec![
        json!({"name":"url","valueUri":url}),
        json!({"name":"system","valueUri":system}),
        json!({"name":"code","valueCode":code}),
    ];
    values.extend(extra);
    json!({"resourceType":"Parameters","parameter":values})
}
async fn validate(app: &axum::Router, url: &str, code: &str, extra: Vec<Value>) -> (u16, Value) {
    request(
        app,
        "POST",
        "/r4/ValueSet/$validate-code",
        parameters(url, SYSTEM, code, extra),
    )
    .await
}
fn assert_member(body: &Value, expected: bool) {
    assert_eq!(
        parameter(body, "result")["valueBoolean"],
        expected,
        "{body}"
    );
}
fn assert_context(body: &Value, version: &str, display: &str) {
    assert_member(body, true);
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
async fn implicit_membership_selects_release_context_without_latest_activation_leakage() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    release(&pool, INTERNATIONAL, "2026-02-01", CURRENT, "Current").await;
    release(&pool, UK_MODULE, "2026-03-01", UK, "UK").await;
    let old = release(&pool, INTERNATIONAL, "2026-01-01", OLD, "Old").await;
    sqlx::query("UPDATE code_systems SET updated_at='2099-01-01' WHERE id=$1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    for (url, expected_version, label) in [
        (whole(SYSTEM), CURRENT, "Current 3"),
        (isa(SYSTEM, "1"), CURRENT, "Current 3"),
        (
            whole(&format!("{SYSTEM}/{INTERNATIONAL}")),
            CURRENT,
            "Current 3",
        ),
        (isa(OLD, "1"), OLD, "Old 3"),
        (whole(UK), UK, "UK 3"),
        (whole(&format!("{SYSTEM}/{UK_MODULE}")), UK, "UK 3"),
    ] {
        let (status, body) = validate(&app, &url, "3", vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_context(&body, expected_version, label);
    }
    for field in ["systemVersion", "valueSetVersion"] {
        let (status, body) = validate(
            &app,
            &whole(SYSTEM),
            "3",
            vec![json!({"name":field,"valueString":OLD})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_context(&body, OLD, "Old 3");
    }
    // An explicit implicit edition/version remains binding; systemVersion is a membership constraint.
    for url in [whole(CURRENT), whole(&format!("{SYSTEM}/{INTERNATIONAL}"))] {
        let (status, body) = validate(
            &app,
            &url,
            "3",
            vec![json!({"name":"systemVersion","valueString":OLD})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_member(&body, false);
    }
    let (status, body) = validate(
        &app,
        &whole(CURRENT),
        "3",
        vec![json!({"name":"valueSetVersion","valueString":OLD})],
    )
    .await;
    assert_eq!(status, 422, "{body}");
    for unavailable in [
        "missing",
        "http://snomed.info/sct/%",
        "http://snomed.info/sct/_",
    ] {
        let (status, body) = validate(
            &app,
            &whole(SYSTEM),
            "3",
            vec![json!({"name":"systemVersion","valueString":unavailable})],
        )
        .await;
        assert_eq!(
            status, 404,
            "Literal versions must not expand as patterns: {body}"
        );
    }
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn implicit_membership_matches_general_inline_semantics_and_explicit_errors() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    let app = api::create_router(store.clone());
    let (status, body) = validate(&app, &whole(SYSTEM), "1", vec![]).await;
    assert_eq!(
        status, 404,
        "No native default release is not a negative membership answer: {body}"
    );
    release(&pool, INTERNATIONAL, "2026-02-01", CURRENT, "Current").await;
    for (url, selection) in [
        (whole(CURRENT), json!({"system":SYSTEM,"version":CURRENT})),
        (
            isa(CURRENT, "1"),
            json!({"system":SYSTEM,"version":CURRENT,"filter":[{"property":"concept","op":"is-a","value":"1"}]}),
        ),
    ] {
        for code in ["1", "2", "3", "4", "5", "missing"] {
            let (status, implicit) = validate(&app, &url, code, vec![]).await;
            assert_eq!(status, 200, "{implicit}");
            let inline =
                json!({"resourceType":"ValueSet","compose":{"include":[selection.clone()]}});
            let (status, general) = request(
                &app,
                "POST",
                "/r4/ValueSet/$validate-code",
                inline_parameters(
                    inline,
                    vec![
                        json!({"name":"system","valueUri":SYSTEM}),
                        json!({"name":"code","valueCode":code}),
                    ],
                ),
            )
            .await;
            assert_eq!(status, 200, "{general}");
            assert_eq!(implicit, general, "{url}, {code}");
        }
    }
    for code in ["4", "5", "missing"] {
        let (status, body) = validate(&app, &isa(CURRENT, "1"), code, vec![]).await;
        assert_eq!(status, 200, "{body}");
        assert_member(&body, false);
    }
    for (display, expected) in [("Current 3", true), ("Wrong display", false)] {
        let (status, body) = validate(
            &app,
            &isa(CURRENT, "1"),
            "3",
            vec![json!({"name":"display","valueString":display})],
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_member(&body, expected);
    }
    let (status, body) = request(
        &app,
        "POST",
        "/r4/ValueSet/$validate-code",
        parameters(&isa(CURRENT, "1"), "urn:another-system", "3", vec![]),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_member(&body, false);
    for (url, code, expected) in [
        (isa(CURRENT, "999999"), "missing", 422),
        (
            whole("http://snomed.info/sct/900000000000207008/version/19990101"),
            "1",
            404,
        ),
        (format!("{CURRENT}?fhir_vs=ecl/<<1"), "1", 422),
        (isa(CURRENT, "1"), "1:111=2", 422),
    ] {
        let (status, body) = validate(&app, &url, code, vec![]).await;
        assert_eq!(status, expected, "{body}");
    }
    let url = whole(CURRENT);
    let result = store
        .query_value_set(
            ValueSetRequest {
                url: &url,
                version: None,
                id: None,
                inline: None,
                system_context: None,
            },
            ValueSetOperation::Find {
                system: SYSTEM,
                version: None,
                code: "5",
            },
        )
        .await
        .unwrap();
    let ValueSetResult::Native(page) = result else {
        panic!("Implicit data must use its native provider")
    };
    assert_eq!(page.members.len(), 1);
    assert!(page.members[0].inactive);
    drop(app);
    drop(store);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn implicit_membership_observes_commits_rollback_and_replacement_after_negative() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, INTERNATIONAL, "2026-02-01", CURRENT, "Current").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let hierarchy = isa(SYSTEM, "1");
    let (status, body) = validate(&app, &hierarchy, "3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, CURRENT, "Current 3");
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("UPDATE snomed_relationships SET destination_code='4' WHERE code_system_id=$1 AND id='3-parent'").bind(id).execute(&mut *writer).await.unwrap();
    writer.rollback().await.unwrap();
    let (status, body) = validate(&app, &hierarchy, "3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, CURRENT, "Current 3");
    sqlx::query("UPDATE snomed_relationships SET destination_code='4' WHERE code_system_id=$1 AND id='3-parent'").bind(id).execute(&pool).await.unwrap();
    let (status, body) = validate(&app, &hierarchy, "3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_member(&body, false);
    sqlx::query("UPDATE snomed_relationships SET destination_code='2' WHERE code_system_id=$1 AND id='3-parent'").bind(id).execute(&pool).await.unwrap();
    sqlx::query("UPDATE concepts SET display='Changed 3' WHERE code_system_id=$1 AND code='3'")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = validate(&app, &hierarchy, "3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, CURRENT, "Changed 3");
    let next = release(&pool, INTERNATIONAL, "2026-04-01", NEXT, "Next").await;
    let (status, body) = validate(&app, &hierarchy, "3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, NEXT, "Next 3");
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(next)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = validate(&app, &hierarchy, "3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, CURRENT, "Changed 3");
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = validate(&app, &whole(SYSTEM), "3", vec![]).await;
    assert_eq!(
        status, 404,
        "A removed release must not retain a stale positive: {body}"
    );
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn implicit_membership_enforces_complete_ancestor_bound_without_traversing_whole_sets() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, INTERNATIONAL, "2026-02-01", CURRENT, "Current").await;
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) SELECT $1,n::text,'Bound '||n,'[]'::jsonb FROM generate_series(10000,18192)n").bind(id).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_relationships SELECT $1,'bound-'||n,'2026-02-01'::date,true,'1','10000',n::text,0,'116680003','900000000000011006','900000000000451002' FROM generate_series(10001,18192)n").bind(id).execute(&pool).await.unwrap();
    sqlx::query("ANALYZE snomed_relationships")
        .execute(&pool)
        .await
        .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = validate(&app, &whole(CURRENT), "10000", vec![]).await;
    assert_eq!(
        status, 200,
        "The implicit whole set does not require an ancestor walk: {body}"
    );
    assert_member(&body, true);
    let (status, body) = validate(&app, &isa(CURRENT, "10001"), "10000", vec![]).await;
    assert_eq!(
        status, 422,
        "An early positive cannot bypass the 8192-node completeness limit: {body}"
    );
    let diagnostic = body["issue"][0]["diagnostics"].as_str().unwrap();
    assert!(
        diagnostic.contains("traversal limit") || diagnostic.contains("8192"),
        "{body}"
    );
    for (system, extra) in [
        ("urn:other", vec![]),
        (
            SYSTEM,
            vec![json!({"name":"systemVersion","valueString":OLD})],
        ),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(&isa(CURRENT, "10001"), system, "10000", extra),
        )
        .await;
        assert_eq!(
            status, 422,
            "A nonmatching system/version cannot bypass the complete traversal guard: {body}"
        );
    }
    let (status, body) = validate(&app, &isa(CURRENT, "10001"), "missing", vec![]).await;
    assert_eq!(
        status, 200,
        "Missing codes must not enumerate a release-sized descendant set: {body}"
    );
    assert_member(&body, false);
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn implicit_membership_never_combines_selected_version_with_another_generation_display() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let old = release(&pool, INTERNATIONAL, "2026-01-01", OLD, "Old").await;
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let mut writer = pool.begin().await.unwrap();
    sqlx::query("LOCK concepts IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let reader = {
        let app = app.clone();
        tokio::spawn(async move { validate(&app, &whole(SYSTEM), "3", vec![]).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let blocked:bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation=to_regclass($1) AND NOT granted)").bind(format!("{schema}.concepts")).fetch_one(&admin).await.unwrap();
            if blocked {break;}
            tokio::task::yield_now().await;
        }
    }).await.expect("Reader must reach the locked projection");
    sqlx::query("UPDATE concepts SET display='Retired 3' WHERE code_system_id=$1 AND code='3'")
        .bind(old)
        .execute(&mut *writer)
        .await
        .unwrap();
    let new = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,content,native_provider) VALUES($1,$2,$3,'active',$4,'snomed-rf2')").bind(new).bind(SYSTEM).bind(CURRENT).bind(sqlx::types::Json(json!({"resourceType":"CodeSystem","url":SYSTEM,"version":CURRENT,"content":"not-present","caseSensitive":true,"hierarchyMeaning":"is-a"}))).execute(&mut *writer).await.unwrap();
    sqlx::query("INSERT INTO snomed_releases VALUES($1,$2,'2026-02-01','900000000000509007','en',repeat('b',64),'{\"generatedFixture\":true}')").bind(new).bind(INTERNATIONAL).execute(&mut *writer).await.unwrap();
    sqlx::query("INSERT INTO concepts(code_system_id,code,display,properties) VALUES($1,'3','Current 3','[]')").bind(new).execute(&mut *writer).await.unwrap();
    writer.commit().await.unwrap();
    let (status, body) = reader.await.unwrap();
    assert_eq!(status, 200, "{body}");
    assert_member(&body, true);
    let context = (
        parameter(&body, "version")["valueString"].as_str().unwrap(),
        parameter(&body, "display")["valueString"].as_str().unwrap(),
    );
    assert!(
        context == (OLD, "Old 3") || context == (CURRENT, "Current 3"),
        "A request may see either complete snapshot, never a mixed pair: {body}"
    );
    let (status, body) = validate(&app, &whole(SYSTEM), "3", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_context(&body, CURRENT, "Current 3");
    drop(app);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn implicit_membership_checks_provider_and_charges_only_matching_payloads() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool, INTERNATIONAL, "2026-02-01", CURRENT, "Current").await;
    sqlx::query(
        "UPDATE concepts SET display=repeat('x',4300000) WHERE code_system_id=$1 AND code='1'",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let app = api::create_router(Arc::new(PostgresStore::new(pool.clone())));
    let (status, body) = validate(&app, &whole(CURRENT), "1", vec![]).await;
    assert_eq!(status, 422, "{body}");
    assert!(body["issue"][0]["diagnostics"]
        .as_str()
        .unwrap()
        .contains("32 MiB"));
    assert!(
        body.to_string().len() < 2048,
        "An oversized member must not be returned inside an error"
    );
    for (system, extra) in [
        ("urn:other", vec![]),
        (
            SYSTEM,
            vec![json!({"name":"systemVersion","valueString":OLD})],
        ),
    ] {
        let (status, body) = request(
            &app,
            "POST",
            "/r4/ValueSet/$validate-code",
            parameters(&whole(CURRENT), system, "1", extra),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_member(&body, false);
    }
    let (status, body) = validate(&app, &whole(CURRENT), "missing", vec![]).await;
    assert_eq!(status, 200, "{body}");
    assert_member(&body, false);
    let (status, body) = validate(&app, &isa(CURRENT, "999999"), "1", vec![]).await;
    assert_eq!(status, 422, "{body}");
    assert!(body["issue"][0]["diagnostics"]
        .as_str()
        .unwrap()
        .contains("Unknown native hierarchy"));
    sqlx::query("UPDATE code_systems SET native_provider=NULL WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, body) = validate(&app, &whole(CURRENT), "missing", vec![]).await;
    assert_eq!(status, 422, "{body}");
    assert!(body["issue"][0]["diagnostics"]
        .as_str()
        .unwrap()
        .contains("RF2 projection"));
    drop(app);
    cleanup(admin, pool, schema).await;
}
