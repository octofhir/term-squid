use anyhow::{ensure, Context, Result};
use sqlx::PgPool;

async fn create(pool: PgPool, file: String, kind: &str) -> Result<()> {
    let content = tokio::fs::read(file)
        .await
        .context("Failed to read resource")?;
    let resource: serde_json::Value = serde_json::from_slice(&content).context("Invalid JSON")?;
    ensure!(resource["resourceType"] == kind, "Unexpected resource type");
    let copy = resource.clone();
    let meta =
        tokio::task::spawn_blocking(move || term_squid_ingestion::inspect_cli(&copy)).await??;
    let mut tx = pool.begin().await?;
    let result = term_squid_ingestion::import_resource(&mut tx, &resource, &meta).await?;
    ensure!(
        !result.unchanged,
        "Resource with this canonical/business version already exists"
    );
    tx.commit().await?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
pub async fn create_code_system(pool: PgPool, file: String) -> Result<()> {
    create(pool, file, "CodeSystem").await
}
pub async fn create_value_set(pool: PgPool, file: String) -> Result<()> {
    create(pool, file, "ValueSet").await
}
pub async fn create_concept_map(pool: PgPool, file: String) -> Result<()> {
    create(pool, file, "ConceptMap").await
}
