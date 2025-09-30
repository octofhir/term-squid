use anyhow::Result;
use sqlx::PgPool;

pub async fn run(pool: PgPool) -> Result<()> {
    println!("\n📚 Installed CodeSystems:");

    let code_systems: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT url, version, status FROM code_systems ORDER BY url, version")
            .fetch_all(&pool)
            .await?;

    for (url, version, status) in code_systems {
        let version_str = version.as_deref().unwrap_or("(no version)");
        println!("  - {url} [{version_str}] - {status}");
    }

    println!("\n📚 Installed ValueSets:");

    let value_sets: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT url, version, status FROM value_sets ORDER BY url, version")
            .fetch_all(&pool)
            .await?;

    for (url, version, status) in value_sets {
        let version_str = version.as_deref().unwrap_or("(no version)");
        println!("  - {url} [{version_str}] - {status}");
    }

    println!("\n📚 Installed ConceptMaps:");

    let concept_maps: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT url, version, status FROM concept_maps ORDER BY url, version")
            .fetch_all(&pool)
            .await?;

    for (url, version, status) in concept_maps {
        let version_str = version.as_deref().unwrap_or("(no version)");
        println!("  - {url} [{version_str}] - {status}");
    }

    println!();

    Ok(())
}
