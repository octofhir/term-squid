//! Bounded HTTP upload admission; archive parsing and publication stay reusable.
use crate::{
    import_resource, inspect, package, ImportResult, Inspection, MAX_CONCEPTS, MAX_JSON_BYTES,
};
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use sqlx::{Postgres, Transaction};
use std::{collections::HashSet, io::Read, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadFormat {
    Json,
    Package,
}
impl UploadFormat {
    pub fn name(self) -> &'static str {
        match self {
            Self::Json => "fhir-json",
            Self::Package => "fhir-package",
        }
    }
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Json => "application/fhir+json",
            Self::Package => "application/gzip",
        }
    }
    pub fn byte_limit(self) -> usize {
        match self {
            Self::Json => MAX_JSON_BYTES,
            Self::Package => package::MAX_PACKAGE_BYTES,
        }
    }
    pub fn from_name(name: &str) -> Result<Self> {
        match name {
            "fhir-json" => Ok(Self::Json),
            "fhir-package" => Ok(Self::Package),
            _ => anyhow::bail!("Unknown upload format"),
        }
    }
}
pub struct InspectedUpload {
    pub inspection: Value,
    pub resources: Vec<(Value, Inspection)>,
    format: UploadFormat,
}

/// No archive member is extracted to disk. Package dependencies are recorded but
/// never fetched; publication is all-or-nothing for the supplied local resources.
pub fn inspect_upload(path: &Path, format: UploadFormat) -> Result<InspectedUpload> {
    if format == UploadFormat::Json {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_JSON_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= MAX_JSON_BYTES, "Input limit exceeded");
        let content: Value = serde_json::from_slice(&bytes)?;
        let meta = inspect(&content)?;
        return Ok(InspectedUpload {
            inspection: serde_json::to_value(&meta)?,
            resources: vec![(content, meta)],
            format,
        });
    }
    let package = package::read_package(path, true)?;
    ensure!(
        !package.name.is_empty()
            && package.name.len() <= 255
            && !package.version.is_empty()
            && package.version.len() <= 100,
        "Invalid package identity"
    );
    let r4 = match package.metadata.get("fhirVersions") {
        Some(Value::Array(versions)) => {
            versions.len() == 1 && versions[0].as_str() == Some("4.0.1")
        }
        None => package.metadata["fhirVersion"].as_str() == Some("4.0.1"),
        _ => false,
    };
    ensure!(
        r4 && package
            .metadata
            .get("fhirVersion")
            .is_none_or(|v| v.as_str() == Some("4.0.1")),
        "Package must declare only FHIR 4.0.1"
    );
    let dependencies = package
        .metadata
        .get("dependencies")
        .cloned()
        .unwrap_or(json!({}));
    let deps = dependencies
        .as_object()
        .context("Invalid package dependencies")?;
    ensure!(
        deps.len() <= 64
            && deps.iter().all(|(name, version)| !name.is_empty()
                && name.len() <= 255
                && version
                    .as_str()
                    .is_some_and(|s| !s.is_empty() && s.len() <= 100)),
        "Invalid package dependencies"
    );
    ensure!(
        !package.resources.is_empty(),
        "Package contains no terminology resources"
    );
    let mut resources = Vec::new();
    let mut identities = HashSet::new();
    let mut records = 0;
    for resource in package.resources {
        let meta = inspect(&resource.content)?;
        records += meta.records;
        ensure!(
            records <= MAX_CONCEPTS,
            "Aggregate package record limit exceeded"
        );
        ensure!(
            identities.insert((
                meta.resource_type.clone(),
                meta.url.clone(),
                meta.version.clone()
            )),
            "Duplicate canonical/business-version in package"
        );
        resources.push((resource.content, meta));
    }
    // Consistent lock ordering across packages avoids opposite-order canonical locks.
    resources.sort_by(|(_, a), (_, b)| {
        (&a.resource_type, &a.url, &a.version).cmp(&(&b.resource_type, &b.url, &b.version))
    });
    let inspection = json!({"resourceType":"FHIRPackage", "url":package.name, "version":package.version,
        "records":records, "recordKind":"concepts and definition entries", "dependencies":dependencies,
        "resources":resources.iter().map(|(_,meta)| meta).collect::<Vec<_>>()});
    Ok(InspectedUpload {
        inspection,
        resources,
        format,
    })
}
impl InspectedUpload {
    /// The caller commits job success and every resource/projection in this transaction.
    pub async fn publish(&self, tx: &mut Transaction<'_, Postgres>) -> Result<Value> {
        let mut results = Vec::new();
        // Acquire every identity in the existing global order before dependency ordering.
        for (_, meta) in &self.resources {
            crate::lock_identity(tx, &meta.resource_type, &meta.url, meta.version.as_deref())
                .await?;
        }
        let mut ordered: Vec<_> = self.resources.iter().collect();
        ordered.sort_by_key(|(content, meta)| {
            if meta.resource_type == "CodeSystem" {
                if content["content"] == "supplement" {
                    1
                } else {
                    0
                }
            } else {
                2
            }
        });
        for (content, meta) in ordered {
            let result: ImportResult = import_resource(tx, content, meta).await?;
            if self.format == UploadFormat::Json {
                return Ok(serde_json::to_value(result)?);
            }
            results.push(json!({"id":result.id,"unchanged":result.unchanged,"resourceType":meta.resource_type,"url":meta.url,"version":meta.version}));
        }
        let unchanged = results.iter().filter(|r| r["unchanged"] == true).count();
        Ok(json!({"resources":results,"imported":results.len()-unchanged,"unchanged":unchanged}))
    }
}
