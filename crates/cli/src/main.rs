mod commands;
mod package;

use clap::{Parser, Subcommand};
use tracing::Level;

#[derive(Parser)]
#[command(name = "term-squid-cli")]
#[command(about = "FHIR Package Loader for term-squid", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Database connection URL
    #[arg(long)]
    database_url: String,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Import a FHIR package from a registry or local file
    Import {
        /// Package name (e.g., hl7.fhir.r4.core) or path to local .tgz file
        package: String,

        /// Package version (e.g., 4.0.1). Not required for local files.
        #[arg(short, long)]
        version: Option<String>,

        /// Dry run - preview what would be imported without making changes
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Import default FHIR packages (R4, R5, R6 core definitions)
    ImportDefaults {
        /// FHIR version to import (r4, r5, r6, or all)
        #[arg(short, long, default_value = "all")]
        version: String,

        /// Dry run - preview what would be imported
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Create a CodeSystem from a FHIR JSON file
    CreateCodeSystem {
        /// Path to FHIR CodeSystem JSON file
        file: String,
    },

    /// Create a ValueSet from a FHIR JSON file
    CreateValueSet {
        /// Path to FHIR ValueSet JSON file
        file: String,
    },

    /// Create a ConceptMap from a FHIR JSON file
    CreateConceptMap {
        /// Path to FHIR ConceptMap JSON file
        file: String,
    },

    /// List installed packages
    List,

    /// Show package statistics
    Stats,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Initialize tracing
    let level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .init();

    // Connect to database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&cli.database_url)
        .await?;

    tracing::info!("Connected to database");

    match cli.command {
        Commands::Import {
            package,
            version,
            dry_run,
            yes,
        } => {
            // Use default FHIR package registry
            let registry = "https://packages.fhir.org".to_string();
            commands::import::run(pool, package, version, registry, dry_run, yes).await?;
        }
        Commands::ImportDefaults {
            version,
            dry_run,
            yes,
        } => {
            commands::import_defaults::run(pool, version, dry_run, yes).await?;
        }
        Commands::CreateCodeSystem { file } => {
            commands::create::create_code_system(pool, file).await?;
        }
        Commands::CreateValueSet { file } => {
            commands::create::create_value_set(pool, file).await?;
        }
        Commands::CreateConceptMap { file } => {
            commands::create::create_concept_map(pool, file).await?;
        }
        Commands::List => {
            commands::list::run(pool).await?;
        }
        Commands::Stats => {
            commands::stats::run(pool).await?;
        }
    }

    Ok(())
}
