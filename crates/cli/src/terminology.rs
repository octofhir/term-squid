// Dynamic test DDL/search paths below use only generated test_<UUID> identifiers.
//! Shared transactional concept/closure import for package and single-resource commands.
use anyhow::{Context, Result};
use sqlx::{Postgres, Transaction};
use term_squid_core::{CodeSystem, PreparedSystem};
use uuid::Uuid;

pub async fn import_concepts(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    content: &serde_json::Value,
) -> Result<()> {
    if content.get("concept").is_none() {
        return Ok(());
    }
    // Preparation validates nested codes and hierarchy before writing anything.
    let resource = content.clone();
    let definition: CodeSystem =
        serde_json::from_value(resource).context("Invalid CodeSystem concepts")?;
    let prepared = PreparedSystem::prepare_for_import(definition)?;
    for batch in prepared.concepts.values().collect::<Vec<_>>().chunks(1000) {
        let mut query = sqlx::QueryBuilder::new(
            "INSERT INTO concepts (code_system_id, code, display, definition, properties) ",
        );
        query.push_values(batch, |mut b, concept| {
            b.push_bind(id)
                .push_bind(&concept.code)
                .push_bind(&concept.display)
                .push_bind(&concept.definition)
                .push_bind(sqlx::types::Json(
                    serde_json::to_value(&concept.property).expect("Properties are serializable"),
                ));
        });
        query.build().execute(&mut **tx).await?;
    }
    if prepared.hierarchy_meaning.as_deref() == Some("is-a") {
        let mut closure = Vec::new();
        for code in prepared.concepts.keys() {
            for (ancestor, depth) in prepared.ancestor_depths(code)? {
                closure.push((ancestor, code.clone(), depth as i32));
                if closure.len() > 1_000_000 {
                    anyhow::bail!("Closure size limit exceeded");
                }
            }
        }
        for batch in closure.chunks(1000) {
            let mut query = sqlx::QueryBuilder::new("INSERT INTO closure_table (code_system_id, ancestor_code, descendant_code, depth) ");
            query.push_values(batch, |mut b, (ancestor, descendant, depth)| {
                b.push_bind(id)
                    .push_bind(ancestor)
                    .push_bind(descendant)
                    .push_bind(*depth);
            });
            query.build().execute(&mut **tx).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::{postgres::PgPoolOptions, PgPool};

    #[tokio::test]
    #[ignore = "requires TERM_SQUID_TEST_DATABASE_URL pointing to a local disposable database"]
    async fn recursive_import_and_closure_are_atomic() {
        let url = std::env::var("TERM_SQUID_TEST_DATABASE_URL").unwrap();
        let admin = PgPool::connect(&url).await.unwrap();
        let schema = format!("test_{}", Uuid::new_v4().simple());
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA {schema}")))
            .execute(&admin)
            .await
            .unwrap();
        let path = format!("SET search_path TO {schema}, public");
        let pool = PgPoolOptions::new()
            .after_connect(move |c, _| {
                let path = path.clone();
                Box::pin(async move {
                    sqlx::query(sqlx::AssertSqlSafe(path)).execute(c).await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap();
        sqlx::migrate!("../backend/migrations")
            .run(&pool)
            .await
            .unwrap();
        let resource = json!({"url":"urn:nested","content":"complete","hierarchyMeaning":"is-a","concept":[
            {"code":"root","concept":[{"code":"child","concept":[{"code":"leaf","property":[{"code":"class","valueCode":"x"}]}]}]}]});
        let id = Uuid::new_v4();
        let mut tx = pool.begin().await.unwrap();
        sqlx::query(
            "INSERT INTO code_systems (id,url,status,content) VALUES ($1,'urn:nested','active',$2)",
        )
        .bind(id)
        .bind(sqlx::types::Json(&resource))
        .execute(&mut *tx)
        .await
        .unwrap();
        import_concepts(&mut tx, id, &resource).await.unwrap();
        tx.commit().await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM concepts WHERE code_system_id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 3);
        let depth: i32 = sqlx::query_scalar("SELECT depth FROM closure_table WHERE code_system_id=$1 AND ancestor_code='root' AND descendant_code='leaf'").bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(depth, 2);
        let invalid = json!({"url":"urn:invalid","content":"complete","concept":[{"code":"a","property":[{"code":"parent","valueCode":"a"}]}]});
        let mut tx = pool.begin().await.unwrap();
        let invalid_id = Uuid::new_v4();
        sqlx::query("INSERT INTO code_systems (id,url,status,content) VALUES ($1,'urn:invalid','active',$2)")
            .bind(invalid_id).bind(sqlx::types::Json(&invalid)).execute(&mut *tx).await.unwrap();
        assert!(import_concepts(&mut tx, invalid_id, &invalid)
            .await
            .is_err());
        tx.rollback().await.unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM code_systems WHERE url='urn:invalid'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0);
        pool.close().await;
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP SCHEMA {schema} CASCADE")))
            .execute(&admin)
            .await
            .unwrap();
        admin.close().await;
    }
}
