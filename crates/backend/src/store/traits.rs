use crate::error::AppError;
use crate::models::{CodeSystem, Concept, ConceptMap, SearchParams, ValueSet};
use async_trait::async_trait;
use serde_json::Value;

/// Trait for terminology storage backend
/// This allows for pluggable storage implementations (PostgreSQL, SQLite, in-memory, etc.)
#[async_trait]
#[allow(dead_code)]
pub trait TerminologyStore: Send + Sync {
    // CodeSystem operations
    async fn create_code_system(&self, cs: CodeSystem) -> Result<CodeSystem, AppError>;
    async fn get_code_system(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<Option<CodeSystem>, AppError>;
    async fn get_code_system_by_id(&self, id: &uuid::Uuid) -> Result<Option<CodeSystem>, AppError>;
    async fn update_code_system(&self, cs: CodeSystem) -> Result<CodeSystem, AppError>;
    async fn delete_code_system(&self, url: &str, version: Option<&str>) -> Result<(), AppError>;
    async fn search_code_systems(&self, params: &SearchParams)
        -> Result<Vec<CodeSystem>, AppError>;

    // ValueSet operations
    async fn create_value_set(&self, vs: ValueSet) -> Result<ValueSet, AppError>;
    async fn get_value_set(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<Option<ValueSet>, AppError>;
    async fn get_value_set_by_id(&self, id: &uuid::Uuid) -> Result<Option<ValueSet>, AppError>;
    async fn update_value_set(&self, vs: ValueSet) -> Result<ValueSet, AppError>;
    async fn delete_value_set(&self, url: &str, version: Option<&str>) -> Result<(), AppError>;
    async fn search_value_sets(&self, params: &SearchParams) -> Result<Vec<ValueSet>, AppError>;

    // ConceptMap operations
    async fn create_concept_map(&self, cm: ConceptMap) -> Result<ConceptMap, AppError>;
    async fn get_concept_map(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<Option<ConceptMap>, AppError>;
    async fn get_concept_map_by_id(&self, id: &uuid::Uuid) -> Result<Option<ConceptMap>, AppError>;
    async fn update_concept_map(&self, cm: ConceptMap) -> Result<ConceptMap, AppError>;
    async fn delete_concept_map(&self, url: &str, version: Option<&str>) -> Result<(), AppError>;
    async fn search_concept_maps(&self, params: &SearchParams)
        -> Result<Vec<ConceptMap>, AppError>;

    // Statistics
    async fn count_code_systems(&self) -> Result<i64, AppError>;
    async fn count_value_sets(&self) -> Result<i64, AppError>;
    async fn count_concept_maps(&self) -> Result<i64, AppError>;

    // Concept operations (for $lookup and $validate-code)
    async fn get_concept(
        &self,
        code_system_id: &uuid::Uuid,
        code: &str,
    ) -> Result<Option<Concept>, AppError>;

    // Subsumption operations (for $subsumes)
    /// Returns Some(true) if code_a subsumes code_b, Some(false) if code_b subsumes code_a, None if no relationship
    async fn check_subsumption(
        &self,
        code_system_id: &uuid::Uuid,
        code_a: &str,
        code_b: &str,
    ) -> Result<Option<bool>, AppError>;

    // ValueSet expansion operations (for $expand)
    async fn get_value_set_expansion(
        &self,
        value_set_id: &uuid::Uuid,
    ) -> Result<Option<Vec<Value>>, AppError>;
}
