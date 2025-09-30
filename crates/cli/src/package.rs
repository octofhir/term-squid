use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;
use tracing::{debug, info};

pub struct PackageDownloader {
    client: reqwest::Client,
    registry_url: String,
}

pub struct FhirPackage {
    pub name: String,
    pub version: String,
    pub resources: Vec<FhirResource>,
}

pub struct FhirResource {
    pub resource_type: String,
    pub url: Option<String>,
    pub content: Value,
}

impl PackageDownloader {
    pub fn new(registry_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            registry_url,
        }
    }

    /// Download a package from the FHIR registry
    pub async fn download(&self, package_name: &str, version: &str) -> Result<PathBuf> {
        let url = format!("{}/{}/{}", self.registry_url, package_name, version);
        info!("Downloading package from: {}", url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to download package")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to download package: HTTP {}", response.status());
        }

        let total_size = response.content_length().unwrap_or(0);
        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                )?
                .progress_chars("#>-"),
        );

        // Create temp file
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("{package_name}-{version}.tgz"));
        let mut file = File::create(&file_path)?;

        // Download with progress
        let mut downloaded = 0u64;
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;
            pb.set_position(downloaded);
        }

        pb.finish_with_message("Downloaded");
        info!("Package downloaded to: {:?}", file_path);

        Ok(file_path)
    }

    /// Extract and parse a package file
    pub fn extract_package(&self, package_path: &Path) -> Result<FhirPackage> {
        info!("Extracting package: {:?}", package_path);

        let file = File::open(package_path)?;
        let decoder = GzDecoder::new(file);
        let mut archive = Archive::new(decoder);

        let mut package_json: Option<Value> = None;
        let mut resources = Vec::new();

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message("Parsing package...");

        for entry in archive.entries()? {
            let mut entry = entry?;
            let path = entry.path()?.to_path_buf();
            let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

            debug!("Processing file: {}", file_name);

            // Read package.json for metadata
            if file_name == "package.json" {
                let mut contents = String::new();
                entry.read_to_string(&mut contents)?;
                package_json = Some(serde_json::from_str(&contents)?);
                continue;
            }

            // Only process JSON files in the package directory
            if !file_name.ends_with(".json") || file_name == "package.json" {
                continue;
            }

            // Parse FHIR resource
            let mut contents = String::new();
            entry.read_to_string(&mut contents)?;

            if let Ok(resource_json) = serde_json::from_str::<Value>(&contents) {
                if let Some(resource_type) =
                    resource_json.get("resourceType").and_then(|v| v.as_str())
                {
                    // Only process terminology resources
                    if matches!(resource_type, "CodeSystem" | "ValueSet" | "ConceptMap") {
                        let resource = FhirResource {
                            resource_type: resource_type.to_string(),
                            url: resource_json
                                .get("url")
                                .and_then(|v| v.as_str())
                                .map(String::from),
                            content: resource_json,
                        };
                        resources.push(resource);
                        pb.set_message(format!("Found {} resources...", resources.len()));
                    }
                }
            }
        }

        pb.finish_with_message(format!("Extracted {} resources", resources.len()));

        let package_metadata = package_json.context("package.json not found in archive")?;
        let name = package_metadata
            .get("name")
            .and_then(|v| v.as_str())
            .context("Package name not found")?
            .to_string();
        let version = package_metadata
            .get("version")
            .and_then(|v| v.as_str())
            .context("Package version not found")?
            .to_string();

        Ok(FhirPackage {
            name,
            version,
            resources,
        })
    }
}

use std::io::Write;
