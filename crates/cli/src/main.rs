mod commands;
mod loinc;
mod package;
mod rxnorm;
mod snomed;
mod terminology;

use clap::{Parser, Subcommand};
use tracing::Level;

#[derive(Parser)]
#[command(name = "term-squid-cli")]
#[command(about = "Terminology content loader for term-squid", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Database connection URL
    #[arg(long)]
    database_url: Option<String>,

    /// Log level (trace, debug, info, warn, error)
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert a scalar between UCUM units using the bundled grammar (no database)
    UcumConvert {
        #[arg(long, allow_hyphen_values = true)]
        value: f64,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Show the lossless canonical affine UCUM representation (no database)
    UcumCanonical {
        #[arg(long)]
        code: String,
    },
    /// Read a password from stdin and print an Argon2id PHC hash (no database required)
    HashAdminPassword,
    /// Apply the repository database migrations before starting the server
    Migrate,
    /// Import a locally supplied complete SNOMED RF2 Snapshot using a checksummed manifest
    ImportSnomed {
        #[arg(long)]
        manifest: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Import a locally supplied complete Loinc.csv main table using a checksummed manifest
    ImportLoinc {
        #[arg(long)]
        manifest: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Import a complete local monthly RXNCONSO.RRF into a current-active projection
    ImportRxnorm {
        #[arg(long)]
        manifest: std::path::PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
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

    if let Commands::UcumConvert { value, from, to } = &cli.command {
        let converted = term_squid_core::ucum::UcumProvider::convert(*value, from, to)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ucumVersion":term_squid_core::ucum::VERSION,"source":from,"target":to,
                "value":value,"result":converted
            }))?
        );
        return Ok(());
    }
    if let Commands::UcumCanonical { code } = &cli.command {
        let canonical = term_squid_core::ucum::UcumProvider::canonical(code)?;
        let mut output = serde_json::to_value(canonical)?;
        output["ucumVersion"] = term_squid_core::ucum::VERSION.into();
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    if matches!(cli.command, Commands::HashAdminPassword) {
        use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
        use std::io::Read;
        let mut password = String::new();
        std::io::stdin().take(4097).read_to_string(&mut password)?;
        let password = password.trim_end_matches(['\r', '\n']);
        anyhow::ensure!(
            (12..=4096).contains(&password.len()),
            "Password must be 12..4096 bytes"
        );
        let salt = SaltString::generate(&mut rand_core::OsRng);
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|_| anyhow::anyhow!("Password hashing failed"))?;
        println!("{hash}");
        return Ok(());
    }
    // Connect to database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(
            &cli.database_url
                .ok_or_else(|| anyhow::anyhow!("--database-url is required"))?,
        )
        .await?;

    tracing::info!("Connected to database");

    match cli.command {
        Commands::UcumConvert { .. } | Commands::UcumCanonical { .. } => unreachable!(),
        Commands::HashAdminPassword => unreachable!(),
        Commands::Migrate => {
            sqlx::migrate!("../backend/migrations").run(&pool).await?;
            println!("Repository migrations applied");
        }
        Commands::ImportSnomed { manifest, dry_run } => {
            let result = snomed::import_manifest(&pool, &manifest, dry_run).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::ImportLoinc { manifest, dry_run } => {
            let result = loinc::import_manifest(&pool, &manifest, dry_run).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Commands::ImportRxnorm { manifest, dry_run } => {
            let result = rxnorm::import_manifest(&pool, &manifest, dry_run).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
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

#[cfg(test)]
mod ucum_command_tests {
    use super::*;
    use term_squid_core::ucum::UcumProvider;

    #[test]
    fn ucum_commands_accept_negative_scalars_and_need_no_database_argument() {
        let cli = Cli::try_parse_from([
            "term-squid-cli",
            "ucum-convert",
            "--value",
            "-40",
            "--from",
            "Cel",
            "--to",
            "[degF]",
        ])
        .unwrap();
        assert!(cli.database_url.is_none());
        let Commands::UcumConvert { value, from, to } = cli.command else {
            panic!("conversion command");
        };
        assert_eq!(value, -40.0);
        assert!((UcumProvider::convert(value, &from, &to).unwrap() + 40.0).abs() < 1e-10);
        let cli =
            Cli::try_parse_from(["term-squid-cli", "ucum-canonical", "--code", "mg"]).unwrap();
        assert!(cli.database_url.is_none());
        let Commands::UcumCanonical { code } = cli.command else {
            panic!("canonical command");
        };
        let canonical = UcumProvider::canonical(&code).unwrap();
        assert_eq!(canonical.unit, "g");
        assert_eq!(canonical.factor, "0.001");
    }

    #[test]
    fn ucum_commands_reject_invalid_and_incompatible_operands_without_database() {
        for (value, from, to) in [
            (1.0, "mg", "m"),
            (1.0, "bad-invalid-unit", "g"),
            (f64::INFINITY, "m", "m"),
        ] {
            assert!(UcumProvider::convert(value, from, to).is_err());
        }
        assert!(UcumProvider::canonical("bad-invalid-unit").is_err());
        assert!(Cli::try_parse_from([
            "term-squid-cli",
            "ucum-convert",
            "--value",
            "not-a-number",
            "--from",
            "m",
            "--to",
            "cm"
        ])
        .is_err());
    }
}
