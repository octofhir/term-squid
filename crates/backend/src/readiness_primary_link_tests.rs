//! A primary-link source declaration is separate from Part registry completeness.
use super::*;

fn source() -> Value {
    json!({"file":"LoincPartLink_Primary.csv","sha256":"a".repeat(64),"rows":5,"bytes":512,"version":"2.82","completeFile":true})
}
async fn release(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,'http://loinc.org','2.82','active','Generated primary link readiness fixture','{}','loinc-csv')").bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO loinc_releases(code_system_id,version,fingerprint,provenance) VALUES($1,'2.82',repeat('a',64),'{\"legacyPrimaryFingerprintSentinel\":true}')").bind(id).execute(pool).await.unwrap();
    id
}
async fn raw(pool: &PgPool, id: Uuid) {
    sqlx::query("INSERT INTO loinc_primary_links VALUES($1,1,'1000-1','Generated name','LP100-0','Generated Part','http://loinc.org','COMPONENT','Primary','http://loinc.org/property/COMPONENT','{}')").bind(id).execute(pool).await.unwrap();
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_primary_links_source_flags_and_privileged_descriptor_drift_fail_closed() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool).await;
    let app = router(pool.clone());
    assert_status(&app, 200).await;
    sqlx::query("UPDATE loinc_releases SET primary_links_loaded=true,primary_link_source=$2 WHERE code_system_id=$1").bind(id).bind(SqlJson(source())).execute(&pool).await.unwrap();
    assert_status(&app, 200).await;
    let mut malformed = vec![json!({}), Value::Null, json!([])];
    for (field, value) in [
        ("file", json!("Part.csv")),
        ("version", json!("2.81")),
        ("sha256", json!("A".repeat(64))),
        ("rows", json!(0)),
        ("rows", json!(20_000_001)),
        ("rows", json!("5")),
        ("bytes", json!(4_294_967_297u64)),
        ("completeFile", json!(false)),
        ("extra", json!(true)),
    ] {
        let mut bad = source();
        bad[field] = value;
        malformed.push(bad);
    }
    let mut missing = source();
    missing.as_object_mut().unwrap().remove("version");
    malformed.push(missing);
    for bad in &malformed {
        let error =
            sqlx::query("UPDATE loinc_releases SET primary_link_source=$2 WHERE code_system_id=$1")
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
    sqlx::query(
        "ALTER TABLE loinc_releases DROP CONSTRAINT loinc_primary_link_source_availability",
    )
    .execute(&pool)
    .await
    .unwrap();
    for bad in malformed {
        sqlx::query("UPDATE loinc_releases SET primary_link_source=$2 WHERE code_system_id=$1")
            .bind(id)
            .bind(SqlJson(bad))
            .execute(&pool)
            .await
            .unwrap();
        assert_status(&app, 503).await;
    }
    sqlx::query("UPDATE loinc_releases SET primary_link_source=$2 WHERE code_system_id=$1")
        .bind(id)
        .bind(SqlJson(source()))
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    sqlx::query("UPDATE loinc_releases SET primary_links_loaded=false WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("UPDATE loinc_releases SET primary_link_source='{}' WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_primary_links_storage_tracking_and_lossless_downgrade_are_required() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool).await;
    let app = router(pool.clone());
    for (remove, restore) in [
        (
            "ALTER TABLE loinc_primary_links RENAME TO unavailable_primary_links",
            "ALTER TABLE unavailable_primary_links RENAME TO loinc_primary_links",
        ),
        (
            "ALTER TABLE loinc_primary_links DISABLE TRIGGER loinc_primary_link_generation",
            "ALTER TABLE loinc_primary_links ENABLE TRIGGER loinc_primary_link_generation",
        ),
    ] {
        sqlx::raw_sql(remove).execute(&pool).await.unwrap();
        assert_status(&app, 503).await;
        sqlx::raw_sql(restore).execute(&pool).await.unwrap();
        assert_status(&app, 200).await;
    }
    sqlx::query("INSERT INTO loinc_terms VALUES($1,'1000-1','{\"rawSentinel\":true}')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let before:Value=sqlx::query_scalar("SELECT to_jsonb(r)-'primary_links_loaded'-'primary_link_source' FROM loinc_releases r WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
    let down = include_str!("../migrations/20261106000003_loinc_primary_links.down.sql");
    sqlx::query("UPDATE loinc_releases SET primary_links_loaded=true,primary_link_source=$2 WHERE code_system_id=$1").bind(id).bind(SqlJson(source())).execute(&pool).await.unwrap();
    for has_raw in [false, true] {
        if has_raw {
            sqlx::query("UPDATE loinc_releases SET primary_links_loaded=false,primary_link_source='{}' WHERE code_system_id=$1").bind(id).execute(&pool).await.unwrap();
            raw(&pool, id).await;
        }
        let mut tx = pool.begin().await.unwrap();
        let error = sqlx::raw_sql(down).execute(&mut *tx).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("P0001")
        );
        tx.rollback().await.unwrap();
    }
    sqlx::query("DELETE FROM loinc_primary_links")
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(down).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM loinc_releases r WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let fields: Value =
        sqlx::query_scalar("SELECT fields FROM loinc_terms WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(fields, json!({"rawSentinel":true}));
    assert_status(&app, 503).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20261106000003_loinc_primary_links.up.sql"
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
async fn readiness_primary_links_downgrade_cannot_remove_a_concurrent_publication() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool).await;
    let mut publisher = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET updated_at=clock_timestamp() WHERE id=$1")
        .bind(id)
        .execute(&mut *publisher)
        .await
        .unwrap();
    sqlx::query("UPDATE loinc_releases SET primary_links_loaded=true,primary_link_source=$2 WHERE code_system_id=$1").bind(id).bind(SqlJson(source())).execute(&mut *publisher).await.unwrap();
    sqlx::query("INSERT INTO loinc_primary_links VALUES($1,1,'1000-1','Generated name','LP100-0','Generated Part','http://loinc.org','COMPONENT','Primary','http://loinc.org/property/COMPONENT','{}')").bind(id).execute(&mut *publisher).await.unwrap();
    let other = pool.clone();
    let task = tokio::spawn(async move {
        let mut tx = other.begin().await.unwrap();
        let error = sqlx::raw_sql(include_str!(
            "../migrations/20261106000003_loinc_primary_links.down.sql"
        ))
        .execute(&mut *tx)
        .await
        .unwrap_err();
        tx.rollback().await.unwrap();
        error
    });
    // Observe the real lock wait instead of relying on scheduling or a fixed sleep.
    tokio::time::timeout(Duration::from_secs(5),async{loop {
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_class c ON c.oid=l.relation JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname='code_systems' AND l.mode='AccessExclusiveLock' AND NOT l.granted)").bind(&schema).fetch_one(&admin).await.unwrap();
        if blocked {break;}tokio::time::sleep(Duration::from_millis(10)).await;
    }}).await.unwrap();
    assert!(!task.is_finished());
    publisher.commit().await.unwrap();
    let error = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("P0001")
    );
    let(loaded,count):(bool,i64)=sqlx::query_as("SELECT r.primary_links_loaded,(SELECT count(*) FROM loinc_primary_links l WHERE l.code_system_id=r.code_system_id) FROM loinc_releases r WHERE r.code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
    assert!(loaded);
    assert_eq!(count, 1);
    assert_status(&router(pool.clone()), 200).await;
    cleanup(admin, pool, schema).await;
}
