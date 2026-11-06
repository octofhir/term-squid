//! Public store paths verify exact hints and queue storage without relaxing snapshots.
use super::*;

async fn source(pool: &PgPool, url: &str, exact: bool) -> Uuid {
    insert_system(pool,json!({"url":url,"version":"1","content":"complete","caseSensitive":exact,"hierarchyMeaning":"is-a","concept":[{"code":"ROOT","concept":[{"code":"A"},{"code":"İ"}]}]})).await
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
async fn exact_hierarchy_hints_still_observe_fresh_generations_and_case_policy_changes() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = source(&pool, "urn:hierarchy-exact-fresh", true).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    assert_eq!(
        store
            .resolve_subsumption_by_id(&id, "ROOT", "A")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    assert!(store
        .resolve_subsumption_by_id(&id, "root", "a")
        .await
        .unwrap()
        .unwrap()
        .code_a
        .is_none());
    let completed = store.generation_stats().1;
    assert_eq!(
        store
            .resolve_subsumption_by_id(&id, "ROOT", "A")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    assert_eq!(
        store.generation_stats().1,
        completed + 1,
        "Exact hierarchy hints still require a new SQL observation"
    );
    let dispatch = store.pause_generation(false).await;
    let mut readers = Vec::new();
    for _ in 0..24 {
        let reader = store.clone();
        readers.push(tokio::spawn(async move {
            reader.resolve_subsumption_by_id(&id, "root", "a").await
        }));
    }
    wait_generation_stats(&store, |stats| stats.2 == 24).await;
    sqlx::query(
        "UPDATE code_systems SET content=jsonb_set(content,'{caseSensitive}','false') WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    drop(dispatch);
    for reader in readers {
        let result = reader.await.unwrap().unwrap().unwrap();
        assert_eq!(result.code_a.as_deref(), Some("ROOT"));
        assert_eq!(result.code_b.as_deref(), Some("A"));
        assert_eq!(result.relationship, Some(true));
    }
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE code_systems SET content=jsonb_set(content,'{caseSensitive}','true') WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&id, "root", "a")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    tx.rollback().await.unwrap();
    sqlx::query(
        "UPDATE code_systems SET content=jsonb_set(content,'{caseSensitive}','true') WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let result = store
        .resolve_subsumption_by_id(&id, "root", "a")
        .await
        .unwrap()
        .unwrap();
    assert!(result.code_a.is_none());
    assert!(result.code_b.is_none());
    assert_eq!(result.relationship, None);
    assert_eq!(
        store
            .resolve_subsumption_by_id(&id, "ROOT", "A")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn exact_hierarchy_delayed_replies_cannot_supply_a_later_folded_request() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = source(&pool, "urn:hierarchy-delayed", true).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    store
        .resolve_subsumption_by_id(&id, "ROOT", "A")
        .await
        .unwrap();
    let completed = store.generation_stats().1;
    let delivery = store.pause_generation(true).await;
    let previous = store.clone();
    let old =
        tokio::spawn(async move { previous.resolve_subsumption_by_id(&id, "ROOT", "A").await });
    wait_generation_stats(&store, |stats| stats.1 > completed).await;
    sqlx::query(
        "UPDATE code_systems SET content=jsonb_set(content,'{caseSensitive}','false') WHERE id=$1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    let next = store.clone();
    let new = tokio::spawn(async move { next.resolve_subsumption_by_id(&id, "root", "a").await });
    wait_generation_stats(&store, |stats| stats.2 == 1).await;
    assert!(!new.is_finished());
    drop(delivery);
    let old = old.await.unwrap().unwrap().unwrap();
    assert_eq!(old.code_a.as_deref(), Some("ROOT"));
    assert_eq!(old.code_b.as_deref(), Some("A"));
    assert_eq!(old.relationship, Some(true));
    let new = new.await.unwrap().unwrap().unwrap();
    assert_eq!(new.code_a.as_deref(), Some("ROOT"));
    assert_eq!(new.code_b.as_deref(), Some("A"));
    assert_eq!(new.relationship, Some(true));
    assert_eq!(
        store.generation_stats().1,
        completed + 2,
        "A request arriving after detach needs its own observation"
    );
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn exact_hierarchy_evicted_weak_hints_fall_back_and_deletions_and_truncation_are_fresh() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = source(&pool, "urn:hierarchy-evicted", true).await;
    let mut others = Vec::new();
    for index in 0..8 {
        others.push(source(&pool, &format!("urn:hierarchy-other-{index}"), true).await);
    }
    let store = Arc::new(PostgresStore::new(pool.clone()));
    store
        .resolve_subsumption_by_id(&id, "ROOT", "A")
        .await
        .unwrap();
    // Cold requests can finish their own SQL observations but wait before loading
    // snapshots. They later evict the only strong owner of the old exact hint.
    let preparation = store.pause_hierarchy_preparation().await;
    let dispatch = store.pause_generation(false).await;
    let completed = store.generation_stats().1;
    let mut cold = Vec::new();
    for other in others {
        let reader = store.clone();
        cold.push(tokio::spawn(async move {
            reader.resolve_subsumption_by_id(&other, "ROOT", "A").await
        }));
    }
    wait_generation_stats(&store, |stats| stats.2 == 8).await;
    drop(dispatch);
    wait_generation_stats(&store, |stats| stats.1 > completed && stats.2 == 0).await;
    let delivery = store.pause_generation(true).await;
    let completed = store.generation_stats().1;
    let reader = store.clone();
    let retired =
        tokio::spawn(async move { reader.resolve_subsumption_by_id(&id, "ROOT", "A").await });
    wait_generation_stats(&store, |stats| stats.1 > completed).await;
    drop(preparation);
    for reader in cold {
        assert_eq!(
            reader.await.unwrap().unwrap().unwrap().relationship,
            Some(true)
        );
    }
    assert_eq!(
        store.hierarchy_cache_usage().await.0,
        1,
        "The ninth snapshot evicts the old eight-entry cache"
    );
    drop(delivery);
    assert_eq!(
        retired.await.unwrap().unwrap().unwrap().relationship,
        Some(true)
    );
    let dispatch = store.pause_generation(false).await;
    let reader = store.clone();
    let deleted =
        tokio::spawn(async move { reader.resolve_subsumption_by_id(&id, "ROOT", "A").await });
    wait_generation_stats(&store, |stats| stats.2 == 1).await;
    sqlx::query("DELETE FROM code_systems WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    drop(dispatch);
    assert!(deleted.await.unwrap().unwrap().is_none());
    let replacement = source(&pool, "urn:hierarchy-replacement", true).await;
    store
        .resolve_subsumption_by_id(&replacement, "ROOT", "A")
        .await
        .unwrap();
    sqlx::query("TRUNCATE code_systems CASCADE")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&replacement, "ROOT", "A")
        .await
        .unwrap()
        .is_none());
    cleanup(admin, pool, schema).await;
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn exact_hierarchy_observer_failure_never_returns_a_warmed_snapshot_or_poisoned_recovery() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let id = source(&pool, "urn:hierarchy-errors", true).await;
    let store = Arc::new(PostgresStore::new(pool.clone()));
    store
        .resolve_subsumption_by_id(&id, "ROOT", "A")
        .await
        .unwrap();
    sqlx::query("UPDATE terminology_generation SET hierarchies_tracked=false")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store
        .resolve_subsumption_by_id(&id, "ROOT", "A")
        .await
        .is_err());
    sqlx::query("UPDATE terminology_generation SET hierarchies_tracked=true")
        .execute(&pool)
        .await
        .unwrap();
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
        &format!("/r4/CodeSystem/{id}/$subsumes?codeA=ROOT&codeB=A"),
        Value::Null,
    )
    .await;
    assert_eq!(status, 500, "{body}");
    assert_eq!(body["resourceType"], "OperationOutcome");
    sqlx::query(
        "ALTER TABLE terminology_generation RENAME COLUMN unavailable_generation TO generation",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
        store
            .resolve_subsumption_by_id(&id, "ROOT", "A")
            .await
            .unwrap()
            .unwrap()
            .relationship,
        Some(true)
    );
    pool.close().await;
    assert!(store
        .resolve_subsumption_by_id(&id, "ROOT", "A")
        .await
        .is_err());
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;
}

async fn mixed_wave(
    store: &Arc<PostgresStore>,
    exact: Uuid,
    folded: Uuid,
    lower_i: &str,
    sql_failure: bool,
) {
    let dispatch = store.pause_generation(false).await;
    let before = store.generation_stats().0;
    let mut readers = Vec::new();
    for index in 0..256 {
        let reader = store.clone();
        let lower_i = lower_i.to_owned();
        readers.push(tokio::spawn(async move {
            match index % 4 {
                0 => {
                    let result = reader.resolve_subsumption_by_id(&exact, "ROOT", "A").await;
                    if sql_failure {
                        assert!(result.is_err());
                    } else {
                        let result = result.unwrap().unwrap();
                        assert_eq!(result.code_a.as_deref(), Some("ROOT"));
                        assert_eq!(result.code_b.as_deref(), Some("A"));
                        assert_eq!(result.relationship, Some(true));
                    }
                }
                1 => {
                    let result = reader
                        .resolve_subsumption_by_id(&folded, "root", &lower_i)
                        .await;
                    if sql_failure {
                        assert!(result.is_err());
                    } else {
                        let result = result.unwrap().unwrap();
                        assert_eq!(result.code_a.as_deref(), Some("ROOT"));
                        assert_eq!(result.code_b.as_deref(), Some("İ"));
                        assert_eq!(result.relationship, Some(true));
                    }
                }
                2 => {
                    let result = reader.resolve_concept_by_id(&exact, "a").await;
                    if sql_failure {
                        assert!(result.is_err());
                    } else {
                        assert!(result.unwrap().unwrap().code.is_none());
                    }
                }
                _ => {
                    let result = reader.expand_compose("urn:set", None).await;
                    if sql_failure {
                        assert!(result.is_err());
                    } else {
                        assert_eq!(result.unwrap().members.values().count(), 3);
                    }
                }
            }
        }));
    }
    wait_generation_stats(store, |stats| stats.2 == 256).await;
    drop(dispatch);
    for reader in readers {
        reader.await.unwrap();
    }
    assert_eq!(
        store.generation_stats().0,
        before + 1,
        "All recipients were admitted before the same detach boundary"
    );
}

#[tokio::test]
#[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
async fn generation_queue_reuses_two_bounded_buffers_across_mixed_batches_and_sql_failure() {
    let (admin, pool, schema) = hierarchy_test_database().await;
    let exact = source(&pool, "urn:hierarchy-buffer-exact", true).await;
    let folded = source(&pool, "urn:hierarchy-buffer-folded", false).await;
    insert_set(
        &pool,
        None,
        json!({"include":[{"system":"urn:hierarchy-buffer-exact"}]}),
    )
    .await;
    let lower_i: String = sqlx::query_scalar("SELECT lower('İ')")
        .fetch_one(&pool)
        .await
        .unwrap();
    let store = Arc::new(PostgresStore::new(pool.clone()));
    store
        .resolve_subsumption_by_id(&exact, "ROOT", "A")
        .await
        .unwrap();
    store
        .resolve_subsumption_by_id(&folded, "root", &lower_i)
        .await
        .unwrap();
    store.resolve_concept_by_id(&exact, "A").await.unwrap();
    store.expand_compose("urn:set", None).await.unwrap();
    mixed_wave(&store, exact, folded, &lower_i, false).await;
    mixed_wave(&store, exact, folded, &lower_i, false).await;
    let capacity = store.generation_buffer_usage();
    assert_eq!((capacity.0, capacity.1), (256, 256));
    assert!(capacity.2 > 0);
    for _ in 0..4 {
        mixed_wave(&store, exact, folded, &lower_i, false).await;
        assert_eq!(
            store.generation_buffer_usage(),
            capacity,
            "Warm queue waves must retain both capacities without another queue allocation"
        );
    }
    sqlx::query(
        "ALTER TABLE terminology_generation RENAME COLUMN generation TO unavailable_generation",
    )
    .execute(&pool)
    .await
    .unwrap();
    mixed_wave(&store, exact, folded, &lower_i, true).await;
    assert_eq!(
        store.generation_buffer_usage(),
        capacity,
        "Failure delivery must return the empty reusable buffer"
    );
    sqlx::query(
        "ALTER TABLE terminology_generation RENAME COLUMN unavailable_generation TO generation",
    )
    .execute(&pool)
    .await
    .unwrap();
    mixed_wave(&store, exact, folded, &lower_i, false).await;
    assert_eq!(store.generation_buffer_usage(), capacity);
    cleanup(admin, pool, schema).await;
}
