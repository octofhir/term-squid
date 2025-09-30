use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{PgPool, Postgres, Transaction};
use std::path::Path;
use tracing::{info, warn};
use uuid::Uuid;

use crate::package::{FhirResource, PackageDownloader};

pub async fn run(
    pool: PgPool,
    package: String,
    version: Option<String>,
    registry: String,
    dry_run: bool,
    yes: bool,
) -> Result<()> {
    info!("Starting package import...");

    let downloader = PackageDownloader::new(registry);

    // Determine if package is a local file or needs to be downloaded
    let package_path = if package.ends_with(".tgz") || package.ends_with(".tar.gz") {
        info!("Using local package file: {}", package);
        Path::new(&package).to_path_buf()
    } else {
        let version = version.context("Version is required when downloading from registry")?;
        downloader.download(&package, &version).await?
    };

    // Extract and parse package
    let fhir_package = downloader.extract_package(&package_path)?;

    info!("Package: {} v{}", fhir_package.name, fhir_package.version);

    // Count resources by type
    let code_systems = fhir_package
        .resources
        .iter()
        .filter(|r| r.resource_type == "CodeSystem")
        .count();
    let value_sets = fhir_package
        .resources
        .iter()
        .filter(|r| r.resource_type == "ValueSet")
        .count();
    let concept_maps = fhir_package
        .resources
        .iter()
        .filter(|r| r.resource_type == "ConceptMap")
        .count();

    println!("\n📦 Package Summary:");
    println!("  Name: {}", fhir_package.name);
    println!("  Version: {}", fhir_package.version);
    println!("  Resources:");
    println!("    - CodeSystems: {code_systems}");
    println!("    - ValueSets: {value_sets}");
    println!("    - ConceptMaps: {concept_maps}");
    println!("    - Total: {}\n", fhir_package.resources.len());

    if dry_run {
        info!("Dry run mode - no changes will be made");
        return Ok(());
    }

    // Confirm import
    if !yes {
        print!("Do you want to proceed with the import? [y/N]: ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            info!("Import cancelled");
            return Ok(());
        }
    }

    // Import resources with transaction
    import_resources(&pool, fhir_package.resources).await?;

    println!("\n✅ Import completed successfully!");

    Ok(())
}

async fn import_resources(pool: &PgPool, resources: Vec<FhirResource>) -> Result<()> {
    let pb = ProgressBar::new(resources.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut tx = pool.begin().await?;

    let mut imported = 0;
    let mut skipped = 0;
    let mut errors = 0;

    for resource in resources {
        pb.set_message(format!(
            "{}: {}",
            resource.resource_type,
            resource.url.as_deref().unwrap_or("unknown")
        ));

        match import_resource(&mut tx, &resource).await {
            Ok(true) => imported += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                warn!("Failed to import resource: {}", e);
                errors += 1;
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message("Import complete");

    if errors > 0 {
        warn!(
            "Import completed with errors: {} imported, {} skipped, {} errors",
            imported, skipped, errors
        );
        tx.rollback().await?;
        anyhow::bail!("Import failed due to errors");
    } else {
        tx.commit().await?;
        info!(
            "Import successful: {} imported, {} skipped",
            imported, skipped
        );
    }

    Ok(())
}

async fn import_resource(
    tx: &mut Transaction<'_, Postgres>,
    resource: &FhirResource,
) -> Result<bool> {
    match resource.resource_type.as_str() {
        "CodeSystem" => import_code_system(tx, resource).await,
        "ValueSet" => import_value_set(tx, resource).await,
        "ConceptMap" => import_concept_map(tx, resource).await,
        _ => Ok(false),
    }
}

async fn import_code_system(
    tx: &mut Transaction<'_, Postgres>,
    resource: &FhirResource,
) -> Result<bool> {
    let url = resource
        .url
        .as_ref()
        .context("CodeSystem must have a url")?;
    let version = resource.content.get("version").and_then(|v| v.as_str());
    let status = resource
        .content
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let name = resource.content.get("name").and_then(|v| v.as_str());
    let title = resource.content.get("title").and_then(|v| v.as_str());

    // Check if already exists
    let exists: bool = if let Some(v) = version {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM code_systems WHERE url = $1 AND version = $2)",
        )
        .bind(url)
        .bind(v)
        .fetch_one(&mut **tx)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM code_systems WHERE url = $1 AND version IS NULL)",
        )
        .bind(url)
        .fetch_one(&mut **tx)
        .await?
    };

    if exists {
        return Ok(false); // Skip existing
    }

    // Insert CodeSystem
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO code_systems (id, url, version, status, name, title, content, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
    )
    .bind(id)
    .bind(url)
    .bind(version)
    .bind(status)
    .bind(name)
    .bind(title)
    .bind(sqlx::types::Json(&resource.content))
    .execute(&mut **tx)
    .await?;

    // Import concepts if present
    if let Some(concepts) = resource.content.get("concept").and_then(|c| c.as_array()) {
        import_concepts(tx, &id, concepts).await?;
    }

    Ok(true)
}

async fn import_concepts(
    tx: &mut Transaction<'_, Postgres>,
    code_system_id: &Uuid,
    concepts: &[serde_json::Value],
) -> Result<()> {
    for concept in concepts {
        let code = concept
            .get("code")
            .and_then(|c| c.as_str())
            .context("Concept must have a code")?;
        let display = concept.get("display").and_then(|d| d.as_str());
        let definition = concept.get("definition").and_then(|d| d.as_str());
        let properties = concept.get("property");

        sqlx::query(
            "INSERT INTO concepts (code_system_id, code, display, definition, properties)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (code_system_id, code) DO NOTHING",
        )
        .bind(code_system_id)
        .bind(code)
        .bind(display)
        .bind(definition)
        .bind(properties.map(sqlx::types::Json))
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn import_value_set(
    tx: &mut Transaction<'_, Postgres>,
    resource: &FhirResource,
) -> Result<bool> {
    let url = resource.url.as_ref().context("ValueSet must have a url")?;
    let version = resource.content.get("version").and_then(|v| v.as_str());
    let status = resource
        .content
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let name = resource.content.get("name").and_then(|v| v.as_str());
    let title = resource.content.get("title").and_then(|v| v.as_str());

    // Check if already exists
    let exists: bool = if let Some(v) = version {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM value_sets WHERE url = $1 AND version = $2)",
        )
        .bind(url)
        .bind(v)
        .fetch_one(&mut **tx)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM value_sets WHERE url = $1 AND version IS NULL)",
        )
        .bind(url)
        .fetch_one(&mut **tx)
        .await?
    };

    if exists {
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO value_sets (id, url, version, status, name, title, content, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(url)
    .bind(version)
    .bind(status)
    .bind(name)
    .bind(title)
    .bind(sqlx::types::Json(&resource.content))
    .execute(&mut **tx)
    .await?;

    Ok(true)
}

async fn import_concept_map(
    tx: &mut Transaction<'_, Postgres>,
    resource: &FhirResource,
) -> Result<bool> {
    let url = resource
        .url
        .as_ref()
        .context("ConceptMap must have a url")?;
    let version = resource.content.get("version").and_then(|v| v.as_str());
    let status = resource
        .content
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Check if already exists
    let exists: bool = if let Some(v) = version {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM concept_maps WHERE url = $1 AND version = $2)",
        )
        .bind(url)
        .bind(v)
        .fetch_one(&mut **tx)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM concept_maps WHERE url = $1 AND version IS NULL)",
        )
        .bind(url)
        .fetch_one(&mut **tx)
        .await?
    };

    if exists {
        return Ok(false);
    }

    sqlx::query(
        "INSERT INTO concept_maps (id, url, version, status, content, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(url)
    .bind(version)
    .bind(status)
    .bind(sqlx::types::Json(&resource.content))
    .execute(&mut **tx)
    .await?;

    Ok(true)
}
