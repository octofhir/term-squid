//! Readiness verifies a fresh application schema, not a clinical coverage score.
use super::*;
use crate::tests::{hierarchy_test_database, request};
use axum::{body::Body, http::Request};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::types::Json as SqlJson;
use tower::ServiceExt;
use uuid::Uuid;
#[path = "readiness_simple_refset_tests.rs"]
mod simple_refsets;

async fn assert_status(app: &Router, expected: u16) -> Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), expected);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        body,
        json!({"status":if expected==200 {"ready"}else{"not-ready"},"service":"term-squid","version":env!("CARGO_PKG_VERSION")})
    );
    body
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
async fn readiness_relationship_source_flags_descriptors_and_loaded_empty_are_strict() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = auxiliary_release(&pool).await;
    let app = router(pool.clone());
    assert_status(&app, 200).await;
    let source = auxiliary_source("RXNREL.RRF", 0);
    sqlx::query("UPDATE rxnorm_releases SET relationships_loaded=true,relationship_source=$2 WHERE code_system_id=$1")
        .bind(id).bind(SqlJson(source.clone())).execute(&pool).await.unwrap();
    assert_status(&app, 200).await;
    let mut malformed = Vec::new();
    for (field, value) in [
        ("file", json!("RXNSAT.RRF")),
        ("version", json!("04062026")),
        ("sha256", json!("A".repeat(64))),
        ("rows", json!(-1)),
        ("rows", json!(20000001)),
        ("rows", json!("0")),
        ("bytes", json!(4294967297u64)),
        ("fullMonthly", json!(false)),
        ("extra", json!(true)),
    ] {
        let mut bad = source.clone();
        bad[field] = value;
        malformed.push(bad);
    }
    malformed.extend([json!({}), Value::Null, json!([])]);
    for bad in &malformed {
        let error = sqlx::query(
            "UPDATE rxnorm_releases SET relationship_source=$2 WHERE code_system_id=$1",
        )
        .bind(id)
        .bind(SqlJson(bad))
        .execute(&pool)
        .await
        .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514"),
            "{bad}: {error}"
        );
    }
    // Readiness independently rejects malformed registrations after privileged drift.
    sqlx::query(
        "ALTER TABLE rxnorm_releases DROP CONSTRAINT rxnorm_relationship_source_availability",
    )
    .execute(&pool)
    .await
    .unwrap();
    for bad in malformed {
        sqlx::query("UPDATE rxnorm_releases SET relationship_source=$2 WHERE code_system_id=$1")
            .bind(id)
            .bind(SqlJson(bad))
            .execute(&pool)
            .await
            .unwrap();
        assert_status(&app, 503).await;
    }
    sqlx::query("UPDATE rxnorm_releases SET relationship_source=$2 WHERE code_system_id=$1")
        .bind(id)
        .bind(SqlJson(source))
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    sqlx::query("UPDATE rxnorm_releases SET relationships_loaded=false WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("UPDATE rxnorm_releases SET relationship_source='{}' WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_relationship_storage_tracking_and_lossless_downgrade_are_required() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = auxiliary_release(&pool).await;
    let app = router(pool.clone());
    for (remove, restore) in [
        (
            "ALTER TABLE rxnorm_relationships RENAME TO unavailable_relationships",
            "ALTER TABLE unavailable_relationships RENAME TO rxnorm_relationships",
        ),
        (
            "ALTER TABLE rxnorm_relationships DISABLE TRIGGER rxnorm_relationship_generation",
            "ALTER TABLE rxnorm_relationships ENABLE TRIGGER rxnorm_relationship_generation",
        ),
    ] {
        sqlx::raw_sql(remove).execute(&pool).await.unwrap();
        assert_status(&app, 503).await;
        sqlx::raw_sql(restore).execute(&pool).await.unwrap();
        assert_status(&app, 200).await;
    }
    let before:Value=sqlx::query_scalar("SELECT to_jsonb(r)-'relationships_loaded'-'relationship_source' FROM rxnorm_releases r WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
    let down = include_str!("../migrations/20261106000002_rxnorm_relationships.down.sql");
    sqlx::query("UPDATE rxnorm_releases SET relationships_loaded=true,relationship_source=$2 WHERE code_system_id=$1")
        .bind(id).bind(SqlJson(auxiliary_source("RXNREL.RRF",0))).execute(&pool).await.unwrap();
    for raw in [false, true] {
        if raw {
            sqlx::query("UPDATE rxnorm_releases SET relationships_loaded=false,relationship_source='{}' WHERE code_system_id=$1").bind(id).execute(&pool).await.unwrap();
            sqlx::query("INSERT INTO rxnorm_relationships VALUES($1,1,'','','CUI','RQ','','','CUI','','','','RXNORM','','','','','','{}')")
                .bind(id).execute(&pool).await.unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        let error = sqlx::raw_sql(down).execute(&mut *tx).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("P0001"),
            "{error}"
        );
        tx.rollback().await.unwrap();
    }
    sqlx::query("DELETE FROM rxnorm_relationships")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(down).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM rxnorm_releases r WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert_status(&app, 503).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20261106000002_rxnorm_relationships.up.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_relationship_downgrade_cannot_remove_a_concurrent_publication() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = auxiliary_release(&pool).await;
    let mut publisher = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET updated_at=clock_timestamp() WHERE id=$1")
        .bind(id)
        .execute(&mut *publisher)
        .await
        .unwrap();
    sqlx::query("UPDATE rxnorm_releases SET relationships_loaded=true,relationship_source=$2 WHERE code_system_id=$1")
        .bind(id).bind(SqlJson(auxiliary_source("RXNREL.RRF",0))).execute(&mut *publisher).await.unwrap();
    let other = pool.clone();
    let task = tokio::spawn(async move {
        let mut tx = other.begin().await.unwrap();
        let error = sqlx::raw_sql(include_str!(
            "../migrations/20261106000002_rxnorm_relationships.down.sql"
        ))
        .execute(&mut *tx)
        .await
        .unwrap_err();
        tx.rollback().await.unwrap();
        error
    });
    tokio::time::sleep(std::time::Duration::from_millis(75)).await;
    assert!(
        !task.is_finished(),
        "Downgrade must wait for the publication transaction"
    );
    publisher.commit().await.unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("P0001")
    );
    let loaded: bool = sqlx::query_scalar(
        "SELECT relationships_loaded FROM rxnorm_releases WHERE code_system_id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(loaded);
    assert_status(&router(pool.clone()), 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
async fn readiness_unavailable_database_is_not_ready_but_liveness_remains_database_free() {
    let pool = crate::database::pool_options(1)
        .connect_lazy(
            "postgresql://secret-user:secret-password@127.0.0.1:1/unavailable-private-database",
        )
        .unwrap();
    pool.close().await;
    let app = crate::api::create_router(Arc::new(crate::store::PostgresStore::new(pool.clone())))
        .merge(router(pool));
    assert_status(&app, 503).await;
    let (status, body) = request(&app, "GET", "/health", Value::Null).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "ok");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_empty_migrated_schema_is_ready_without_auth_tables_or_inventory_mutations() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    sqlx::raw_sql("ALTER TABLE admin_sessions RENAME TO private_sessions_not_used; ALTER TABLE admin_login_limit RENAME TO private_login_limit_not_used; ALTER TABLE admin_import_jobs RENAME TO private_jobs_not_used").execute(&pool).await.unwrap();
    let before:(i64,i64,i64,i64,i32)=sqlx::query_as("SELECT generation,(SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concept_maps),(SELECT attempts FROM private_login_limit_not_used) FROM terminology_generation WHERE singleton").fetch_one(&pool).await.unwrap();
    let app = router(pool.clone());
    for _ in 0..3 {
        assert_status(&app, 200).await;
    }
    let response = app
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .is_empty());
    let after:(i64,i64,i64,i64,i32)=sqlx::query_as("SELECT generation,(SELECT count(*) FROM code_systems),(SELECT count(*) FROM value_sets),(SELECT count(*) FROM concept_maps),(SELECT attempts FROM private_login_limit_not_used) FROM terminology_generation WHERE singleton").fetch_one(&pool).await.unwrap();
    assert_eq!(
        before, after,
        "Readiness must never mutate content, generations or authentication state"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_requires_exact_successful_compiled_migrations_and_recovers_without_restart() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let app = router(pool.clone());
    assert_status(&app, 200).await;
    let migration = MIGRATIONS
        .iter()
        .find(|m| m.migration_type.is_up_migration())
        .unwrap();
    for sql in [
        "UPDATE _sqlx_migrations SET success=false WHERE version=$1",
        "UPDATE _sqlx_migrations SET checksum=decode(repeat('00',48),'hex') WHERE version=$1",
        "DELETE FROM _sqlx_migrations WHERE version=$1",
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(sql)
            .bind(migration.version)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_status(&app, 200).await;
        tx.rollback().await.unwrap();
        assert_status(&app, 200).await;
        let row:(i64,String,bool,Vec<u8>,i64)=sqlx::query_as("SELECT version,description,success,checksum,execution_time FROM _sqlx_migrations WHERE version=$1").bind(migration.version).fetch_one(&pool).await.unwrap();
        sqlx::query(sql)
            .bind(migration.version)
            .execute(&pool)
            .await
            .unwrap();
        assert_status(&app, 503).await;
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version=$1")
            .bind(migration.version)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES($1,$2,$3,$4,$5)").bind(row.0).bind(row.1).bind(row.2).bind(row.3).bind(row.4).execute(&pool).await.unwrap();
        assert_status(&app, 200).await;
    }
    sqlx::query("INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES($1,'unknown future migration',true,decode(repeat('00',48),'hex'),0)").bind(i64::MAX).execute(&pool).await.unwrap();
    assert_status(&app, 503).await;
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version=$1")
        .bind(i64::MAX)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_rejects_missing_schema_guards_and_generation_triggers_then_recovers() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let app = router(pool.clone());
    assert_status(&app, 200).await;
    for sql in [
        "UPDATE terminology_generation SET maps_tracked=false",
        "UPDATE terminology_generation SET hierarchies_tracked=false",
        "UPDATE terminology_generation SET generation=0",
        "DELETE FROM terminology_generation",
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
        assert_status(&app, 200).await;
        tx.commit().await.unwrap();
        assert_status(&app, 503).await;
        sqlx::query("INSERT INTO terminology_generation(singleton,generation,maps_tracked,hierarchies_tracked) VALUES(true,1,true,true) ON CONFLICT(singleton) DO UPDATE SET generation=1,maps_tracked=true,hierarchies_tracked=true").execute(&pool).await.unwrap();
        assert_status(&app, 200).await;
    }
    sqlx::query("ALTER TABLE value_sets DISABLE TRIGGER value_set_generation")
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("ALTER TABLE value_sets ENABLE TRIGGER value_set_generation")
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    sqlx::query("DROP TRIGGER rxnorm_atom_generation ON rxnorm_atoms")
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("CREATE TRIGGER rxnorm_atom_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rxnorm_atoms FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation()").execute(&pool).await.unwrap();
    assert_status(&app, 200).await;
    sqlx::query("ALTER TABLE terminology_generation RENAME TO missing_generation")
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("ALTER TABLE missing_generation RENAME TO terminology_generation")
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_native_provider_registration_requires_matching_canonical_release_metadata() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let app = router(pool.clone());
    for(provider,url,version,publication)in[
        ("snomed-rf2","http://snomed.info/sct","http://snomed.info/sct/900000000000207008/version/20260201","INSERT INTO snomed_releases VALUES($1,'900000000000207008','2026-02-01','900000000000509007','en',repeat('a',64),'{}')"),
        ("loinc-csv","http://loinc.org","2.82","INSERT INTO loinc_releases VALUES($1,'2.82',repeat('a',64),'{}')"),
        ("rxnorm-rrf","http://www.nlm.nih.gov/research/umls/rxnorm","03022026","INSERT INTO rxnorm_releases VALUES($1,'03022026','2026-03-02',repeat('a',64),'{}')"),
    ] {
        let id=Uuid::new_v4();
        sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,$2,$3,'active','Generated readiness fixture',$4,$5)")
            .bind(id).bind(url).bind(version).bind(SqlJson(json!({"resourceType":"CodeSystem","url":url,"version":version,"content":"not-present"}))).bind(provider).execute(&pool).await.unwrap();
        assert_status(&app,503).await;
        sqlx::query(publication).bind(id).execute(&pool).await.unwrap();
        assert_status(&app,200).await;
        if provider=="rxnorm-rrf" {
            let mut tx=pool.begin().await.unwrap();
            sqlx::query("UPDATE rxnorm_releases SET release_date='2026-03-03' WHERE code_system_id=$1")
                .bind(id).execute(&mut *tx).await.unwrap();
            assert_status(&app,200).await;
            tx.rollback().await.unwrap();
            assert_status(&app,200).await;
            sqlx::query("UPDATE rxnorm_releases SET release_date='2026-03-03' WHERE code_system_id=$1")
                .bind(id).execute(&pool).await.unwrap();
            assert_status(&app,503).await;
            sqlx::query("UPDATE rxnorm_releases SET release_date='2026-03-02' WHERE code_system_id=$1")
                .bind(id).execute(&pool).await.unwrap();
            assert_status(&app,200).await;
        }
        let mut tx=pool.begin().await.unwrap();
        sqlx::query("UPDATE code_systems SET version='wrong-version' WHERE id=$1").bind(id).execute(&mut *tx).await.unwrap();
        assert_status(&app,200).await;
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE code_systems SET version='wrong-version' WHERE id=$1").bind(id).execute(&pool).await.unwrap();
        assert_status(&app,503).await;
        sqlx::query("UPDATE code_systems SET version=$2 WHERE id=$1").bind(id).bind(version).execute(&pool).await.unwrap();
        assert_status(&app,200).await;
        sqlx::query("UPDATE code_systems SET version=NULL WHERE id=$1").bind(id).execute(&pool).await.unwrap();
        assert_status(&app,503).await;
        sqlx::query("UPDATE code_systems SET version=$2 WHERE id=$1").bind(id).bind(version).execute(&pool).await.unwrap();
        assert_status(&app,200).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_probe_capacity_cancellation_checkout_deadline_and_recovery_are_bounded() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let state = Readiness {
        pool: pool.clone(),
        admission: Arc::new(Semaphore::new(PROBES)),
    };
    let app = routes(state.clone());
    assert_status(&app, 200).await;
    let mut held = Vec::new();
    for _ in 0..5 {
        held.push(pool.acquire().await.unwrap());
    }
    let mut tasks = Vec::new();
    for _ in 0..PROBES {
        let app = app.clone();
        tasks.push(tokio::spawn(async move { assert_status(&app, 503).await }));
    }
    tokio::time::timeout(Duration::from_secs(1), async {
        while state.admission.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), assert_status(&app, 503))
        .await
        .unwrap();
    for task in tasks {
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
    }
    assert_eq!(state.admission.available_permits(), PROBES);
    let start = tokio::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(3), assert_status(&app, 503))
        .await
        .unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(state.admission.available_permits(), PROBES);
    drop(held);
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_statement_deadline_is_local_and_does_not_poison_the_pool() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let app = router(pool.clone());
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE code_systems IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), assert_status(&app, 503))
        .await
        .unwrap();
    lock.rollback().await.unwrap();
    assert_status(&app, 200).await;
    let timeout: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        timeout, "0",
        "Readiness must not change later terminology statement budgets"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_native_metadata_inventory_has_an_explicit_nonpartial_bound() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) SELECT gen_random_uuid(),'http://loinc.org',n::text,'active','Generated readiness bound fixture',jsonb_build_object('resourceType','CodeSystem','url','http://loinc.org','version',n::text,'content','not-present'),'loinc-csv' FROM generate_series(1,$1::bigint)n")
        .bind(NATIVE_LIMIT).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases SELECT id,version,repeat('a',64),'{\"generatedFixture\":true}'::jsonb FROM code_systems")
        .execute(&pool).await.unwrap();
    let app = router(pool.clone());
    assert_status(&app, 200).await;
    let extra = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,'http://loinc.org','overflow','active','Generated readiness overflow fixture','{}','loinc-csv')")
        .bind(extra).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases VALUES($1,'overflow',repeat('a',64),'{\"generatedFixture\":true}')")
        .bind(extra).execute(&pool).await.unwrap();
    assert_status(&app, 503).await;
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(extra)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

fn auxiliary_source(file: &str, rows: u64) -> Value {
    json!({"file":file,"sha256":"a".repeat(64),"rows":rows,"bytes":rows*100,"version":"03022026","fullMonthly":true})
}

async fn auxiliary_release(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,'http://www.nlm.nih.gov/research/umls/rxnorm','03022026','active','Generated auxiliary readiness fixture','{}','rxnorm-rrf')")
        .bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO rxnorm_releases(code_system_id,version,release_date,fingerprint,provenance) VALUES($1,'03022026','2026-03-02',repeat('a',64),'{\"legacyConsoFingerprintSentinel\":true}')")
        .bind(id).execute(pool).await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_auxiliary_availability_is_explicit_and_rejects_malformed_source_cohorts() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = auxiliary_release(&pool).await;
    let app = router(pool.clone());
    for (sty, sat) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut sources = json!({});
        if sty {
            sources["RXNSTY.RRF"] = auxiliary_source("RXNSTY.RRF", 1);
        }
        if sat {
            sources["RXNSAT.RRF"] = auxiliary_source("RXNSAT.RRF", 0);
        }
        sqlx::query("UPDATE rxnorm_releases SET semantic_types_loaded=$2,attributes_loaded=$3,auxiliary_sources=$4 WHERE code_system_id=$1")
            .bind(id).bind(sty).bind(sat).bind(SqlJson(sources)).execute(&pool).await.unwrap();
        // An explicitly loaded empty SAT source is valid. Readiness verifies cohort
        // metadata, not clinical-row counts or per-concept attribute completeness.
        assert_status(&app, 200).await;
    }
    let valid = json!({"RXNSTY.RRF":auxiliary_source("RXNSTY.RRF",1),"RXNSAT.RRF":auxiliary_source("RXNSAT.RRF",0)});
    let mut invalid = vec![
        json!({}),
        json!([]),
        json!({"RXNSTY.RRF":auxiliary_source("RXNSTY.RRF",1)}),
    ];
    for (field, value) in [
        ("file", json!("wrong-file.RRF")),
        ("sha256", json!("A".repeat(64))),
        ("rows", json!(-1)),
        ("rows", json!(1.5)),
        ("rows", json!(20_000_001)),
        ("bytes", json!(4_294_967_297_u64)),
        ("bytes", json!(-1)),
        ("version", json!("04062026")),
        ("fullMonthly", json!(false)),
        ("fullMonthly", json!("true")),
        ("unexpected", json!("not ignored")),
    ] {
        let mut source = valid.clone();
        source["RXNSAT.RRF"][field] = value;
        invalid.push(source);
    }
    let mut missing = valid.clone();
    missing["RXNSAT.RRF"]
        .as_object_mut()
        .unwrap()
        .remove("sha256");
    invalid.push(missing);
    let mut empty_sty = valid.clone();
    empty_sty["RXNSTY.RRF"]["rows"] = 0.into();
    invalid.push(empty_sty);
    let mut foreign_file = valid.clone();
    foreign_file["RXNREL.RRF"] = json!({});
    invalid.push(foreign_file);
    for source in invalid {
        let error =
            sqlx::query("UPDATE rxnorm_releases SET auxiliary_sources=$2 WHERE code_system_id=$1")
                .bind(id)
                .bind(SqlJson(source))
                .execute(&pool)
                .await
                .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
        assert_status(&app, 200).await;
    }
    let error =
        sqlx::query("UPDATE rxnorm_releases SET attributes_loaded=false WHERE code_system_id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    // The route still fails closed if a privileged operator removed the CHECK.
    sqlx::query("ALTER TABLE rxnorm_releases DROP CONSTRAINT rxnorm_auxiliary_source_availability")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE rxnorm_releases SET auxiliary_sources='{}' WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("UPDATE rxnorm_releases SET auxiliary_sources=$2 WHERE code_system_id=$1")
        .bind(id)
        .bind(SqlJson(valid))
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_requires_auxiliary_tables_and_enabled_generation_tracking() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let app = router(pool.clone());
    assert_status(&app, 200).await;
    for (missing,restore) in [
        ("ALTER TABLE rxnorm_semantic_types RENAME TO unavailable_semantic_types","ALTER TABLE unavailable_semantic_types RENAME TO rxnorm_semantic_types"),
        ("ALTER TABLE rxnorm_attributes RENAME TO unavailable_attributes","ALTER TABLE unavailable_attributes RENAME TO rxnorm_attributes"),
        ("ALTER TABLE rxnorm_semantic_types DISABLE TRIGGER rxnorm_semantic_type_generation","ALTER TABLE rxnorm_semantic_types ENABLE TRIGGER rxnorm_semantic_type_generation"),
        ("DROP TRIGGER rxnorm_attribute_generation ON rxnorm_attributes","CREATE TRIGGER rxnorm_attribute_generation AFTER INSERT OR UPDATE OR DELETE OR TRUNCATE ON rxnorm_attributes FOR EACH STATEMENT EXECUTE FUNCTION advance_terminology_generation()"),
    ] {
        sqlx::query(missing).execute(&pool).await.unwrap();
        assert_status(&app,503).await;
        sqlx::query(restore).execute(&pool).await.unwrap();
        assert_status(&app,200).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_auxiliary_migration_downgrade_preserves_legacy_cohorts_but_refuses_loaded_or_raw_sources(
) {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = auxiliary_release(&pool).await;
    let before:Value=sqlx::query_scalar("SELECT to_jsonb(r)-'semantic_types_loaded'-'attributes_loaded'-'auxiliary_sources' FROM rxnorm_releases r WHERE code_system_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    let down = include_str!("../migrations/20261106000001_rxnorm_auxiliary.down.sql");
    sqlx::query("UPDATE rxnorm_releases SET attributes_loaded=true,auxiliary_sources=$2 WHERE code_system_id=$1")
        .bind(id).bind(SqlJson(json!({"RXNSAT.RRF":auxiliary_source("RXNSAT.RRF",0)}))).execute(&pool).await.unwrap();
    for kind in ["loaded-empty-sat", "unclaimed-sty", "unclaimed-sat"] {
        let mut tx = pool.begin().await.unwrap();
        let error = sqlx::raw_sql(down).execute(&mut *tx).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("P0001"),
            "{kind}: {error}"
        );
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE rxnorm_releases SET attributes_loaded=false,auxiliary_sources='{}' WHERE code_system_id=$1").bind(id).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM rxnorm_semantic_types")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM rxnorm_attributes")
            .execute(&pool)
            .await
            .unwrap();
        if kind == "loaded-empty-sat" {
            sqlx::query("INSERT INTO rxnorm_semantic_types VALUES($1,1,'1','T200','A1.3.3','Clinical Drug','{}')").bind(id).execute(&pool).await.unwrap();
        } else if kind == "unclaimed-sty" {
            sqlx::query("INSERT INTO rxnorm_attributes VALUES($1,1,'','','RUI','','','NDC','RXNORM','raw unsupported relationship attribute','N','{}')").bind(id).execute(&pool).await.unwrap();
        }
    }
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(down).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM rxnorm_releases r WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "Downgrade must not rewrite CONSO-only identity or provenance"
    );
    let app = router(pool.clone());
    assert_status(&app, 503).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20261106000001_rxnorm_auxiliary.up.sql"
    ))
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_status(&app, 200).await;
    let restored:(bool,bool,SqlJson<Value>)=sqlx::query_as("SELECT semantic_types_loaded,attributes_loaded,auxiliary_sources FROM rxnorm_releases WHERE code_system_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!((restored.0, restored.1), (false, false));
    assert_eq!(restored.2 .0, json!({}));
    let restored:Value=sqlx::query_scalar("SELECT to_jsonb(r)-'semantic_types_loaded'-'attributes_loaded'-'auxiliary_sources' FROM rxnorm_releases r WHERE code_system_id=$1")
        .bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(before, restored);
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_auxiliary_downgrade_serializes_before_a_concurrent_publication_check() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = auxiliary_release(&pool).await;
    let mut publication = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE code_systems SET name='Publication holds the first persistent table' WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *publication)
    .await
    .unwrap();
    sqlx::query("UPDATE rxnorm_releases SET attributes_loaded=true,auxiliary_sources=$2 WHERE code_system_id=$1")
        .bind(id).bind(SqlJson(json!({"RXNSAT.RRF":auxiliary_source("RXNSAT.RRF",0)}))).execute(&mut *publication).await.unwrap();
    let worker = pool.clone();
    let downgrade = tokio::spawn(async move {
        let mut tx = worker.begin().await.unwrap();
        let result = sqlx::raw_sql(include_str!(
            "../migrations/20261106000001_rxnorm_auxiliary.down.sql"
        ))
        .execute(&mut *tx)
        .await;
        tx.rollback().await.unwrap();
        result
    });
    tokio::time::timeout(Duration::from_secs(3),async {
        loop {
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_class c ON c.oid=l.relation JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname='code_systems' AND NOT l.granted)")
                .bind(&schema).fetch_one(&pool).await.unwrap();
            if waiting {break;}
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    publication.commit().await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(3), downgrade)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("P0001")
    );
    let loaded: bool =
        sqlx::query_scalar("SELECT attributes_loaded FROM rxnorm_releases WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        loaded,
        "The committed source cohort must survive a refused downgrade"
    );
    assert_status(&router(pool.clone()), 200).await;
    cleanup(admin, pool, schema).await;
}

#[path = "readiness_primary_link_tests.rs"]
mod primary_link_tests;
