use anyhow::{Context, Result};
use sqlx::PgPool;
use std::fs;
use tracing::info;
use uuid::Uuid;

/// Create a CodeSystem from a FHIR JSON file
pub async fn create_code_system(pool: PgPool, file_path: String) -> Result<()> {
    info!("Creating CodeSystem from file: {}", file_path);

    // Read and parse the JSON file
    let content =
        fs::read_to_string(&file_path).context(format!("Failed to read file: {file_path}"))?;

    let json: serde_json::Value = serde_json::from_str(&content).context("Failed to parse JSON")?;

    // Validate resource type
    let resource_type = json["resourceType"]
        .as_str()
        .context("Missing resourceType field")?;

    if resource_type != "CodeSystem" {
        anyhow::bail!("Expected resourceType 'CodeSystem', got '{resource_type}'");
    }

    // Extract required fields
    let url = json["url"]
        .as_str()
        .context("Missing required field 'url'")?
        .to_string();

    let status = json["status"]
        .as_str()
        .context("Missing required field 'status'")?
        .to_string();

    let version = json["version"].as_str().map(|s| s.to_string());
    let name = json["name"].as_str().map(|s| s.to_string());
    let title = json["title"].as_str().map(|s| s.to_string());
    let fhir_version = json["fhirVersion"].as_str().map(|s| s.to_string());

    // Check if already exists
    let exists: bool = if let Some(v) = &version {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM code_systems WHERE url = $1 AND version = $2)",
        )
        .bind(&url)
        .bind(v)
        .fetch_one(&pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM code_systems WHERE url = $1 AND version IS NULL)",
        )
        .bind(&url)
        .fetch_one(&pool)
        .await?
    };

    if exists {
        anyhow::bail!("CodeSystem with url '{url}' and version '{version:?}' already exists");
    }

    // Insert CodeSystem
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO code_systems (id, url, version, status, name, title, fhir_version, content, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())",
    )
    .bind(id)
    .bind(&url)
    .bind(&version)
    .bind(&status)
    .bind(&name)
    .bind(&title)
    .bind(&fhir_version)
    .bind(sqlx::types::Json(&json))
    .execute(&pool)
    .await?;

    // Import concepts if present
    if let Some(concepts) = json.get("concept").and_then(|c| c.as_array()) {
        let concept_count = concepts.len();
        info!("Importing {} concepts...", concept_count);

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
            .bind(id)
            .bind(code)
            .bind(display)
            .bind(definition)
            .bind(properties.map(sqlx::types::Json))
            .execute(&pool)
            .await?;
        }
    }

    println!("✅ CodeSystem created successfully!");
    println!("  ID: {id}");
    println!("  URL: {url}");
    if let Some(v) = version {
        println!("  Version: {v}");
    }

    Ok(())
}

/// Create a ValueSet from a FHIR JSON file
pub async fn create_value_set(pool: PgPool, file_path: String) -> Result<()> {
    info!("Creating ValueSet from file: {}", file_path);

    // Read and parse the JSON file
    let content =
        fs::read_to_string(&file_path).context(format!("Failed to read file: {file_path}"))?;

    let json: serde_json::Value = serde_json::from_str(&content).context("Failed to parse JSON")?;

    // Validate resource type
    let resource_type = json["resourceType"]
        .as_str()
        .context("Missing resourceType field")?;

    if resource_type != "ValueSet" {
        anyhow::bail!("Expected resourceType 'ValueSet', got '{resource_type}'");
    }

    // Extract required fields
    let url = json["url"]
        .as_str()
        .context("Missing required field 'url'")?
        .to_string();

    let status = json["status"]
        .as_str()
        .context("Missing required field 'status'")?
        .to_string();

    let version = json["version"].as_str().map(|s| s.to_string());
    let name = json["name"].as_str().map(|s| s.to_string());
    let title = json["title"].as_str().map(|s| s.to_string());
    let fhir_version = json["fhirVersion"].as_str().map(|s| s.to_string());

    // Check if already exists
    let exists: bool = if let Some(v) = &version {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM value_sets WHERE url = $1 AND version = $2)",
        )
        .bind(&url)
        .bind(v)
        .fetch_one(&pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM value_sets WHERE url = $1 AND version IS NULL)",
        )
        .bind(&url)
        .fetch_one(&pool)
        .await?
    };

    if exists {
        anyhow::bail!("ValueSet with url '{url}' and version '{version:?}' already exists");
    }

    // Insert ValueSet
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO value_sets (id, url, version, status, name, title, fhir_version, content, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())",
    )
    .bind(id)
    .bind(&url)
    .bind(&version)
    .bind(&status)
    .bind(&name)
    .bind(&title)
    .bind(&fhir_version)
    .bind(sqlx::types::Json(&json))
    .execute(&pool)
    .await?;

    println!("✅ ValueSet created successfully!");
    println!("  ID: {id}");
    println!("  URL: {url}");
    if let Some(v) = version {
        println!("  Version: {v}");
    }

    Ok(())
}

/// Create a ConceptMap from a FHIR JSON file
pub async fn create_concept_map(pool: PgPool, file_path: String) -> Result<()> {
    info!("Creating ConceptMap from file: {}", file_path);

    // Read and parse the JSON file
    let content =
        fs::read_to_string(&file_path).context(format!("Failed to read file: {file_path}"))?;

    let json: serde_json::Value = serde_json::from_str(&content).context("Failed to parse JSON")?;

    // Validate resource type
    let resource_type = json["resourceType"]
        .as_str()
        .context("Missing resourceType field")?;

    if resource_type != "ConceptMap" {
        anyhow::bail!("Expected resourceType 'ConceptMap', got '{resource_type}'");
    }

    // Extract required fields
    let url = json["url"]
        .as_str()
        .context("Missing required field 'url'")?
        .to_string();

    let status = json["status"]
        .as_str()
        .context("Missing required field 'status'")?
        .to_string();

    let version = json["version"].as_str().map(|s| s.to_string());
    let name = json["name"].as_str().map(|s| s.to_string());
    let title = json["title"].as_str().map(|s| s.to_string());
    let fhir_version = json["fhirVersion"].as_str().map(|s| s.to_string());

    let source_uri = json["sourceUri"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| json["sourceCanonical"].as_str().map(|s| s.to_string()));

    let target_uri = json["targetUri"]
        .as_str()
        .map(|s| s.to_string())
        .or_else(|| json["targetCanonical"].as_str().map(|s| s.to_string()));

    // Check if already exists
    let exists: bool = if let Some(v) = &version {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM concept_maps WHERE url = $1 AND version = $2)",
        )
        .bind(&url)
        .bind(v)
        .fetch_one(&pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM concept_maps WHERE url = $1 AND version IS NULL)",
        )
        .bind(&url)
        .fetch_one(&pool)
        .await?
    };

    if exists {
        anyhow::bail!("ConceptMap with url '{url}' and version '{version:?}' already exists");
    }

    // Insert ConceptMap
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO concept_maps (id, url, version, status, name, title, fhir_version, source_uri, target_uri, content, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())",
    )
    .bind(id)
    .bind(&url)
    .bind(&version)
    .bind(&status)
    .bind(&name)
    .bind(&title)
    .bind(&fhir_version)
    .bind(&source_uri)
    .bind(&target_uri)
    .bind(sqlx::types::Json(&json))
    .execute(&pool)
    .await?;

    println!("✅ ConceptMap created successfully!");
    println!("  ID: {id}");
    println!("  URL: {url}");
    if let Some(v) = version {
        println!("  Version: {v}");
    }

    Ok(())
}
