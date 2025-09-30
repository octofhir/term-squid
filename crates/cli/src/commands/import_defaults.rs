use anyhow::Result;
use sqlx::PgPool;
use tracing::info;

pub async fn run(pool: PgPool, version: String, dry_run: bool, yes: bool) -> Result<()> {
    info!("Import defaults for version: {}", version);

    let packages = match version.to_lowercase().as_str() {
        "r4" => vec![("hl7.fhir.r4.core", "4.0.1")],
        "r5" => vec![("hl7.fhir.r5.core", "5.0.0")],
        "r6" => vec![("hl7.fhir.r6.core", "6.0.0-ballot2")],
        "all" => vec![
            ("hl7.fhir.r4.core", "4.0.1"),
            ("hl7.fhir.r5.core", "5.0.0"),
            ("hl7.fhir.r6.core", "6.0.0-ballot2"),
        ],
        _ => anyhow::bail!("Invalid version: {version}. Must be r4, r5, r6, or all"),
    };

    for (package_name, package_version) in packages {
        info!("Importing {} v{}", package_name, package_version);

        super::import::run(
            pool.clone(),
            package_name.to_string(),
            Some(package_version.to_string()),
            "https://packages.fhir.org".to_string(),
            dry_run,
            yes,
        )
        .await?;
    }

    Ok(())
}
