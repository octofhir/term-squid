use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

// Simplified FHIR resource models for storage
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CodeSystem {
    pub id: Uuid,
    pub url: String,
    pub version: Option<String>,
    pub status: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub fhir_version: Option<String>,
    pub content: Json<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ValueSet {
    pub id: Uuid,
    pub url: String,
    pub version: Option<String>,
    pub status: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub fhir_version: Option<String>,
    pub content: Json<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConceptMap {
    pub id: Uuid,
    pub url: String,
    pub version: Option<String>,
    pub status: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub source_uri: Option<String>,
    pub target_uri: Option<String>,
    pub fhir_version: Option<String>,
    pub content: Json<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Concept {
    pub id: Uuid,
    pub code_system_id: Uuid,
    pub code: String,
    pub display: Option<String>,
    pub definition: Option<String>,
    pub properties: Option<Json<serde_json::Value>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// Search parameters
#[derive(Debug, Default, Clone)]
pub struct SearchParams {
    pub url: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    pub fhir_version: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// One bounded row for lookup/validation; deliberately excludes the resource JSON.
#[derive(Debug, sqlx::FromRow)]
pub struct ResolvedConcept {
    pub url: String,
    pub native_provider: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub content_mode: Option<String>,
    pub code: Option<String>,
    pub display: Option<String>,
    pub definition: Option<String>,
    pub properties: Option<Json<serde_json::Value>>,
}

/// System metadata, normalized codes and hierarchy resolved in one database snapshot.
#[derive(Debug, sqlx::FromRow)]
pub struct ResolvedSubsumption {
    /// Projection observations are usable without claiming a complete authoritative hierarchy.
    pub complete: bool,
    pub url: String,
    pub version: Option<String>,
    pub hierarchy_meaning: Option<String>,
    pub code_a: Option<String>,
    pub code_b: Option<String>,
    /// Some(true): A subsumes B; Some(false): B subsumes A; None: no strict relationship.
    pub relationship: Option<bool>,
}

/// Resource and members resolved from the same database snapshot.
#[derive(Clone)]
pub struct ExpandedValueSet {
    pub resource: std::sync::Arc<serde_json::Value>,
    pub members: std::sync::Arc<term_squid_core::PreparedValueSet>,
    pub sources: std::sync::Arc<ExpansionSources>,
    pub knowledge: std::sync::Arc<
        std::collections::BTreeMap<term_squid_core::Canonical, term_squid_core::CodeSystemIndex>,
    >,
}

/// Actual selected dependency editions, including dependencies of an empty result.
#[derive(Clone, Debug, Default)]
pub struct ExpansionSources {
    pub systems: std::collections::BTreeSet<term_squid_core::Canonical>,
    pub value_sets: std::collections::BTreeSet<term_squid_core::Canonical>,
    pub supplements: std::collections::BTreeSet<term_squid_core::Canonical>,
}
impl ExpansionSources {
    pub fn parameters(&self) -> Vec<serde_json::Value> {
        let mut result = Vec::new();
        for (name, values) in [
            ("used-codesystem", &self.systems),
            ("used-valueset", &self.value_sets),
            ("used-supplement", &self.supplements),
        ] {
            result.extend(values.iter().map(|key| {
                let value = key.version.as_ref().map_or_else(
                    || key.url.clone(),
                    |version| format!("{}|{version}", key.url),
                );
                serde_json::json!({"name":name,"valueUri":value})
            }));
        }
        result
    }
    pub fn memory_charge(&self) -> usize {
        self.systems
            .iter()
            .chain(&self.value_sets)
            .chain(&self.supplements)
            .map(|key| key.url.len() + key.version.as_ref().map_or(0, String::len) + 256)
            .sum()
    }
}
