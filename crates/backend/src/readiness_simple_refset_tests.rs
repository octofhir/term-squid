//! Declared concept-refset registries are bounded metadata, not a clinical inventory.
use super::*;

fn source() -> Value {
    json!({"file":"der2_Refset_SimpleSnapshot_INT_20260201.txt","sha256":"d".repeat(64),"rows":0,"bytes":70,"release":"20260201","completeSnapshot":true,"conceptOnly":true,"refsetIds":["700001","700002"]})
}
async fn release(pool: &PgPool) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO code_systems(id,url,version,status,name,content,native_provider) VALUES($1,'http://snomed.info/sct','http://snomed.info/sct/900000000000207008/version/20260201','active','Generated simple refset readiness fixture','{}','snomed-rf2')").bind(id).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO snomed_releases(code_system_id,edition_module,release_date,dialect,language,fingerprint,provenance) VALUES($1,'900000000000207008','2026-02-01','900000000000509007','en',repeat('a',64),'{\"legacyRefsetSentinel\":true}')").bind(id).execute(pool).await.unwrap();
    id
}
async fn loaded(pool: &PgPool, id: Uuid) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE snomed_releases SET simple_refsets_loaded=true,simple_refset_source=$2 WHERE code_system_id=$1").bind(id).bind(SqlJson(source())).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO snomed_simple_refsets VALUES($1,'700001'),($1,'700002')")
        .bind(id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_simple_refset_sources_validate_descriptor_shape_and_declared_empty_registries() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool).await;
    let app = router(pool.clone());
    assert_status(&app, 200).await;
    loaded(&pool, id).await;
    assert_status(&app, 200).await;
    let mut bad_sources = vec![json!({}), json!([]), Value::Null];
    for (field, value) in [
        ("file", json!("SimpleFull_20260201.txt")),
        ("release", json!("20260301")),
        ("sha256", json!("bad")),
        ("rows", json!(-1)),
        ("rows", json!(20_000_001)),
        ("bytes", json!(2_147_483_649u64)),
        ("completeSnapshot", json!(false)),
        ("conceptOnly", json!(false)),
        ("refsetIds", json!([])),
        ("refsetIds", json!(["700002", "700001"])),
        ("refsetIds", json!(["700001", "700001"])),
        ("refsetIds", json!(["0700001"])),
        ("refsetIds", json!([700001])),
        ("refsetIds", json!([null])),
        ("refsetIds", json!({"700001":true})),
        ("extra", json!(true)),
    ] {
        let mut value_source = source();
        value_source[field] = value;
        bad_sources.push(value_source);
    }
    for bad in &bad_sources {
        let error = sqlx::query(
            "UPDATE snomed_releases SET simple_refset_source=$2 WHERE code_system_id=$1",
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
    sqlx::query(
        "ALTER TABLE snomed_releases DROP CONSTRAINT snomed_simple_refset_source_availability",
    )
    .execute(&pool)
    .await
    .unwrap();
    for bad in bad_sources {
        sqlx::query("UPDATE snomed_releases SET simple_refset_source=$2 WHERE code_system_id=$1")
            .bind(id)
            .bind(SqlJson(bad))
            .execute(&pool)
            .await
            .unwrap();
        assert_status(&app, 503).await;
    }
    sqlx::query("UPDATE snomed_releases SET simple_refset_source=$2 WHERE code_system_id=$1")
        .bind(id)
        .bind(SqlJson(source()))
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_simple_refset_registry_metadata_and_generation_drift_fail_closed() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool).await;
    let app = router(pool.clone());
    loaded(&pool, id).await;
    assert_status(&app, 200).await;
    sqlx::query("DELETE FROM snomed_simple_refsets WHERE code_system_id=$1 AND refset_id='700002'")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("INSERT INTO snomed_simple_refsets VALUES($1,'700002')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    sqlx::query("INSERT INTO snomed_simple_refsets VALUES($1,'700003')")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 503).await;
    sqlx::query("DELETE FROM snomed_simple_refsets WHERE code_system_id=$1 AND refset_id='700003'")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert_status(&app, 200).await;
    for (remove, restore) in [
        (
            "ALTER TABLE snomed_simple_refsets RENAME TO unavailable_refsets",
            "ALTER TABLE unavailable_refsets RENAME TO snomed_simple_refsets",
        ),
        (
            "ALTER TABLE snomed_simple_members RENAME TO unavailable_members",
            "ALTER TABLE unavailable_members RENAME TO snomed_simple_members",
        ),
        (
            "ALTER TABLE snomed_simple_refsets DISABLE TRIGGER snomed_simple_refset_generation",
            "ALTER TABLE snomed_simple_refsets ENABLE TRIGGER snomed_simple_refset_generation",
        ),
        (
            "ALTER TABLE snomed_simple_members DISABLE TRIGGER snomed_simple_member_generation",
            "ALTER TABLE snomed_simple_members ENABLE TRIGGER snomed_simple_member_generation",
        ),
    ] {
        sqlx::raw_sql(remove).execute(&pool).await.unwrap();
        assert_status(&app, 503).await;
        sqlx::raw_sql(restore).execute(&pool).await.unwrap();
        assert_status(&app, 200).await;
    }
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn readiness_simple_refset_downgrade_is_lossless_and_refuses_declared_or_unclaimed_rows() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool).await;
    let app = router(pool.clone());
    let before:Value=sqlx::query_scalar("SELECT to_jsonb(r)-'simple_refsets_loaded'-'simple_refset_source' FROM snomed_releases r WHERE code_system_id=$1").bind(id).fetch_one(&pool).await.unwrap();
    let down = include_str!("../migrations/20261106000004_snomed_simple_refsets.down.sql");
    loaded(&pool, id).await;
    for unclaimed in [false, true] {
        if unclaimed {
            sqlx::query("UPDATE snomed_releases SET simple_refsets_loaded=false,simple_refset_source='{}' WHERE code_system_id=$1").bind(id).execute(&pool).await.unwrap();
        }
        let mut tx = pool.begin().await.unwrap();
        let error = sqlx::raw_sql(down).execute(&mut *tx).await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("P0001")
        );
        tx.rollback().await.unwrap();
    }
    sqlx::query("DELETE FROM snomed_simple_refsets WHERE code_system_id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(down).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let after: Value =
        sqlx::query_scalar("SELECT to_jsonb(r) FROM snomed_releases r WHERE code_system_id=$1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert_status(&app, 503).await;
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20261106000004_snomed_simple_refsets.up.sql"
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
async fn readiness_simple_refset_downgrade_waits_for_an_actual_registry_publication() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = release(&pool).await;
    let mut publisher = pool.begin().await.unwrap();
    sqlx::query("UPDATE code_systems SET updated_at=clock_timestamp() WHERE id=$1")
        .bind(id)
        .execute(&mut *publisher)
        .await
        .unwrap();
    sqlx::query("UPDATE snomed_releases SET simple_refsets_loaded=true,simple_refset_source=$2 WHERE code_system_id=$1").bind(id).bind(SqlJson(source())).execute(&mut *publisher).await.unwrap();
    sqlx::query("INSERT INTO snomed_simple_refsets VALUES($1,'700001'),($1,'700002')")
        .bind(id)
        .execute(&mut *publisher)
        .await
        .unwrap();
    let other = pool.clone();
    let task = tokio::spawn(async move {
        let mut tx = other.begin().await.unwrap();
        let error = sqlx::raw_sql(include_str!(
            "../migrations/20261106000004_snomed_simple_refsets.down.sql"
        ))
        .execute(&mut *tx)
        .await
        .unwrap_err();
        tx.rollback().await.unwrap();
        error
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE relation='code_systems'::regclass AND mode='AccessExclusiveLock' AND NOT granted)").fetch_one(&pool).await.unwrap();
        if waiting {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Downgrade did not wait for publication"
        );
        tokio::task::yield_now().await;
    }
    publisher.commit().await.unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("P0001")
    );
    assert_status(&router(pool.clone()), 200).await;
    cleanup(admin, pool, schema).await;
}
