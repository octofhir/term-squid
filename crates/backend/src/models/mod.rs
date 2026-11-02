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
    pub id: Uuid,
    pub hierarchy_meaning: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub content_mode: Option<String>,
    pub code: Option<String>,
    pub display: Option<String>,
    pub definition: Option<String>,
    pub properties: Option<Json<serde_json::Value>>,
}

/// Resource and members resolved from the same database snapshot.
pub struct ExpandedValueSet {
    pub resource: serde_json::Value,
    pub members: term_squid_core::Members,
}
