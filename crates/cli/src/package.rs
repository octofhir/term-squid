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

        if name == "hl7.fhir.r4.core" && version == "4.0.1" {
            normalize_legacy_r4_tables(&mut resources);
        }

        Ok(FhirPackage {
            name,
            version,
            resources,
        })
    }
}

// This immutable published package has six v2 tables with pipe-versioned URLs and
// table numbers in the version element. Keep the workaround at the package boundary:
// ordinary resources and operation parameters must still reject version conflicts.
fn legacy_r4_table(url: &str) -> Option<(&str, &str, &str)> {
    let (base, version) = url.split_once('|')?;
    let table = base.strip_prefix("http://terminology.hl7.org/CodeSystem/v2-")?;
    match (table, version) {
        ("0006", "2.1" | "2.4") | ("0360", "2.3.1" | "2.7") | ("0391", "2.4" | "2.6") => {
            Some((base, version, table))
        }
        _ => None,
    }
}

fn normalize_legacy_r4_tables(resources: &mut [FhirResource]) {
    let mut changed = 0;
    for resource in resources {
        match resource.resource_type.as_str() {
            "CodeSystem" => {
                let Some((base, version, table)) =
                    resource.url.as_deref().and_then(legacy_r4_table)
                else {
                    continue;
                };
                if resource.content["version"].as_str() != Some(table) {
                    continue;
                }
                let (base, version) = (base.to_owned(), version.to_owned());
                resource.content["url"] = base.clone().into();
                resource.content["version"] = version.into();
                resource.url = Some(base);
                changed += 1;
            }
            "ValueSet" => {
                for mode in ["include", "exclude"] {
                    let Some(selections) = resource
                        .content
                        .get_mut("compose")
                        .and_then(|compose| compose.get_mut(mode))
                        .and_then(Value::as_array_mut)
                    else {
                        continue;
                    };
                    for selection in selections {
                        let Some((base, version, _)) =
                            selection["system"].as_str().and_then(legacy_r4_table)
                        else {
                            continue;
                        };
                        if selection.get("version").is_some_and(|v| !v.is_null()) {
                            continue;
                        }
                        let (base, version) = (base.to_owned(), version.to_owned());
                        selection["system"] = base.into();
                        selection["version"] = version.into();
                        changed += 1;
                    }
                }
            }
            _ => {}
        }
    }
    if changed > 0 {
        info!(
            "Normalized {changed} legacy v2 table definitions/references in hl7.fhir.r4.core#4.0.1"
        );
    }
}

use std::io::Write;

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use serde_json::json;

    fn extract(name: &str, version: &str, resources: Vec<Value>) -> FhirPackage {
        let directory =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.benchmarks/package-tests");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("{}.tgz", uuid::Uuid::new_v4()));
        let mut archive = tar::Builder::new(GzEncoder::new(
            File::create(&path).unwrap(),
            Compression::default(),
        ));
        for (index, value) in std::iter::once(json!({"name":name,"version":version}))
            .chain(resources)
            .enumerate()
        {
            let bytes = serde_json::to_vec(&value).unwrap();
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            let filename = if index == 0 {
                "package/package.json".to_owned()
            } else {
                format!("package/resource-{index}.json")
            };
            archive
                .append_data(&mut header, filename, bytes.as_slice())
                .unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap();
        // Extraction is local; do not consult macOS proxy settings in this fixture.
        let downloader = PackageDownloader {
            client: reqwest::Client::builder().no_proxy().build().unwrap(),
            registry_url: "http://unused.invalid".into(),
        };
        let result = downloader.extract_package(&path);
        std::fs::remove_file(path).unwrap();
        result.unwrap()
    }

    #[test]
    fn r4_core_legacy_table_versions_are_normalized_at_extraction() {
        for (table, versions) in [
            ("0006", ["2.1", "2.4"]),
            ("0360", ["2.3.1", "2.7"]),
            ("0391", ["2.4", "2.6"]),
        ] {
            let base = format!("http://terminology.hl7.org/CodeSystem/v2-{table}");
            for version in versions {
                let legacy = format!("{base}|{version}");
                let package = extract(
                    "hl7.fhir.r4.core",
                    "4.0.1",
                    vec![
                        json!({"resourceType":"CodeSystem","url":legacy,"version":table,"content":"complete","concept":[{"code":"x"}]}),
                        json!({"resourceType":"ValueSet","url":"urn:set","version":"1","compose":{
                        "include":[{"system":legacy}],"exclude":[{"system":legacy,"concept":[{"code":"x"}]}]}}),
                    ],
                );
                let system = &package.resources[0];
                assert_eq!(system.url.as_deref(), Some(base.as_str()));
                assert_eq!(system.content["url"], base);
                assert_eq!(system.content["version"], version);
                let prepared = term_squid_core::PreparedSystem::prepare_for_import(
                    serde_json::from_value(system.content.clone()).unwrap(),
                )
                .unwrap();
                assert_eq!(prepared.canonical.url, base);
                assert_eq!(prepared.canonical.version.as_deref(), Some(version));
                for mode in ["include", "exclude"] {
                    let selection = &package.resources[1].content["compose"][mode][0];
                    assert_eq!(selection["system"], base);
                    assert_eq!(selection["version"], version);
                }
            }
        }
    }

    #[test]
    fn legacy_normalization_does_not_rewrite_other_packages_or_conflicts() {
        let legacy = "http://terminology.hl7.org/CodeSystem/v2-0006|2.4";
        let resource =
            json!({"resourceType":"CodeSystem","url":legacy,"version":"0006","content":"complete"});
        for (name, version) in [
            ("org.example.local", "4.0.1"),
            ("hl7.fhir.r4.core", "4.0.2"),
        ] {
            let package = extract(name, version, vec![resource.clone()]);
            assert_eq!(package.resources[0].content, resource);
            assert_eq!(package.resources[0].url.as_deref(), Some(legacy));
        }
        let resources = vec![
            json!({"resourceType":"CodeSystem","url":legacy,"version":"conflict","content":"complete"}),
            json!({"resourceType":"CodeSystem","url":"urn:local|1","version":"2","content":"complete"}),
            json!({"resourceType":"ValueSet","url":"urn:set","compose":{"include":[{"system":legacy,"version":"conflict"}]}}),
            json!({"resourceType":"CodeSystem","url":"urn:local","version":"1","content":"complete"}),
            json!({"resourceType":"ValueSet","url":"urn:expansion-only","expansion":{"contains":[]}}),
            json!({"resourceType":"CodeSystem","url":"http://terminology.hl7.org/CodeSystem/v2-0006|2.8","version":"0006","content":"complete"}),
        ];
        let package = extract("hl7.fhir.r4.core", "4.0.1", resources.clone());
        for (actual, expected) in package.resources.iter().zip(resources) {
            assert_eq!(actual.content, expected);
        }
        assert!(term_squid_core::Canonical::parse(legacy, Some("conflict")).is_err());
    }
}
