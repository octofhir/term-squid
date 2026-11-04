//! FHIR npm-package parsing shared by CLI downloads and protected local uploads.
use anyhow::{ensure, Context, Result};
use flate2::read::{GzDecoder, MultiGzDecoder};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs::File,
    io::{self, Read},
    path::{Component, Path},
};
use tar::Archive;
use tracing::info;

pub const MAX_PACKAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_PACKAGE_DECOMPRESSED: u64 = 128 * 1024 * 1024;
pub const MAX_PACKAGE_RESOURCES: usize = 256;
const MAX_ENTRIES: usize = 4096;

pub struct FhirPackage {
    pub name: String,
    pub version: String,
    pub resources: Vec<FhirResource>,
    pub metadata: Value,
}
pub struct FhirResource {
    pub resource_type: String,
    pub url: Option<String>,
    pub content: Value,
}

/// The CLI retains its established permissive package selection/normalization.
/// HTTP applies a separate bounded, R4-only admission policy to the same parser.
pub fn read_package(path: &Path, bounded: bool) -> Result<FhirPackage> {
    let file = File::open(path)?;
    if bounded {
        ensure!(
            file.metadata()?.len() <= MAX_PACKAGE_BYTES as u64,
            "Compressed package limit exceeded"
        );
    }
    let reader: Box<dyn Read> = if bounded {
        Box::new(BudgetReader {
            inner: MultiGzDecoder::new(BudgetReader {
                inner: file,
                remaining: MAX_PACKAGE_BYTES as u64,
            }),
            remaining: MAX_PACKAGE_DECOMPRESSED,
        })
    } else {
        Box::new(GzDecoder::new(file))
    };
    let mut archive = Archive::new(reader);
    let mut metadata = None;
    let mut resources = Vec::new();
    let mut paths = HashSet::new();
    let mut selected_bytes = 0u64;
    for (index, entry) in archive.entries()?.enumerate() {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if bounded {
            ensure!(index < MAX_ENTRIES, "Archive entry limit exceeded");
            ensure!(
                path.as_os_str().len() <= 512
                    && path.components().all(|c| matches!(c, Component::Normal(_))),
                "Unsafe archive path"
            );
            ensure!(
                path.components()
                    .next()
                    .is_some_and(|c| c.as_os_str() == "package"),
                "Expected package/ archive layout"
            );
            ensure!(paths.insert(path.clone()), "Duplicate archive path");
            ensure!(
                entry.header().entry_type().is_file() || entry.header().entry_type().is_dir(),
                "Archive links and special entries are unsupported"
            );
            if entry.header().entry_type().is_dir() {
                continue;
            }
        }
        let is_metadata = if bounded {
            path == Path::new("package/package.json")
        } else {
            filename == "package.json"
        };
        let is_json = filename.ends_with(".json")
            && (!bounded || path.parent() == Some(Path::new("package")));
        if !is_metadata && !is_json {
            continue;
        }
        if bounded {
            let limit = if is_metadata {
                1024 * 1024
            } else {
                crate::MAX_JSON_BYTES
            };
            ensure!(
                entry.size() <= limit as u64,
                "Package member limit exceeded"
            );
            selected_bytes += entry.size();
            ensure!(
                selected_bytes <= MAX_PACKAGE_BYTES as u64,
                "Package JSON byte limit exceeded"
            );
        }
        let mut contents = String::new();
        entry.read_to_string(&mut contents)?;
        let parsed = serde_json::from_str::<Value>(&contents);
        if is_metadata {
            ensure!(!bounded || metadata.is_none(), "Duplicate package metadata");
            metadata = Some(parsed?);
        } else if let Ok(content) = parsed {
            if let Some(kind @ ("CodeSystem" | "ValueSet" | "ConceptMap")) =
                content["resourceType"].as_str()
            {
                ensure!(
                    !bounded || resources.len() < MAX_PACKAGE_RESOURCES,
                    "Package resource limit exceeded"
                );
                resources.push(FhirResource {
                    resource_type: kind.into(),
                    url: content["url"].as_str().map(String::from),
                    content,
                });
            }
        } else if bounded {
            anyhow::bail!("Invalid JSON package member");
        }
    }
    // tar stops at its end marker; drain gzip as well to verify CRC and the full
    // decompression budget, including ignored files and trailing compressed data.
    io::copy(&mut archive.into_inner(), &mut io::sink())?;
    let metadata = metadata.context("package.json not found in archive")?;
    let name = metadata["name"]
        .as_str()
        .context("Package name not found")?
        .to_owned();
    let version = metadata["version"]
        .as_str()
        .context("Package version not found")?
        .to_owned();
    if name == "hl7.fhir.r4.core" && version == "4.0.1" {
        normalize_legacy_r4_tables(&mut resources);
    }
    Ok(FhirPackage {
        name,
        version,
        resources,
        metadata,
    })
}

struct BudgetReader<R> {
    inner: R,
    remaining: u64,
}
impl<R: Read> Read for BudgetReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return match self.inner.read(&mut [0])? {
                0 => Ok(0),
                _ => Err(io::Error::other("Package decompression limit exceeded")),
            };
        }
        let limit = output.len().min(self.remaining as usize);
        let read = self.inner.read(&mut output[..limit])?;
        self.remaining -= read as u64;
        Ok(read)
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
