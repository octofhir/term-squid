use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::path::{Path, PathBuf};
use tracing::info;

pub struct PackageDownloader {
    client: reqwest::Client,
    registry_url: String,
}

pub use term_squid_ingestion::package::{FhirPackage, FhirResource};

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

    /// Parse a local package through the shared ingestion service.
    pub fn extract_package(&self, package_path: &Path) -> Result<FhirPackage> {
        term_squid_ingestion::package::read_package(package_path, false)
    }
}

use std::io::Write;

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use serde_json::{json, Value};

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
