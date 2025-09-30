use anyhow::Result;
use sqlx::PgPool;

pub async fn run(pool: PgPool) -> Result<()> {
    let code_systems: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM code_systems")
        .fetch_one(&pool)
        .await?;

    let value_sets: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM value_sets")
        .fetch_one(&pool)
        .await?;

    let concept_maps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM concept_maps")
        .fetch_one(&pool)
        .await?;

    let concepts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM concepts")
        .fetch_one(&pool)
        .await?;

    println!("\n📊 Terminology Server Statistics:");
    println!("  CodeSystems: {code_systems}");
    println!("  ValueSets: {value_sets}");
    println!("  ConceptMaps: {concept_maps}");
    println!("  Concepts: {concepts}");
    println!();

    Ok(())
}
