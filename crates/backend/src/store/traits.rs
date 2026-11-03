use crate::error::AppError;
use crate::models::{
    CodeSystem, Concept, ConceptMap, ExpandedValueSet, ResolvedConcept, ResolvedSubsumption,
    SearchParams, ValueSet,
};
use async_trait::async_trait;
use serde_json::Value;

/// Trait for terminology storage backend
/// This allows for pluggable storage implementations (PostgreSQL, SQLite, in-memory, etc.)
#[async_trait]
// async-trait adds must_use to an already must-use boxed Future on newer Clippy.
#[allow(dead_code, clippy::double_must_use)]
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

    async fn prepare_concept_map(
        &self,
        url: &str,
        version: Option<&str>,
        id: Option<uuid::Uuid>,
    ) -> Result<std::sync::Arc<term_squid_core::PreparedConceptMap>, AppError>;

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

    async fn resolve_concept(
        &self,
        url: &str,
        version: Option<&str>,
        code: &str,
    ) -> Result<Option<ResolvedConcept>, AppError>;
    async fn resolve_concept_by_id(
        &self,
        id: &uuid::Uuid,
        code: &str,
    ) -> Result<Option<ResolvedConcept>, AppError>;
    async fn expand_compose(
        &self,
        url: &str,
        version: Option<&str>,
    ) -> Result<ExpandedValueSet, AppError>;

    async fn expand_compose_by_id(&self, id: &uuid::Uuid) -> Result<ExpandedValueSet, AppError>;

    /// Prepare an ephemeral root using one fresh snapshot of its stored dependencies.
    async fn compose_inline(
        &self,
        resource: &Value,
        value_set_version: Option<&str>,
        system_context: Option<(&str, &str)>,
    ) -> Result<ExpandedValueSet, AppError>;

    async fn compose_for_validation(
        &self,
        url: &str,
        version: Option<&str>,
        id: Option<uuid::Uuid>,
        system: &str,
        system_version: Option<&str>,
    ) -> Result<ExpandedValueSet, AppError>;

    async fn query_value_set(
        &self,
        request: super::ValueSetRequest<'_>,
        operation: super::ValueSetOperation<'_>,
    ) -> Result<super::ValueSetResult, AppError> {
        super::value_set::finite(self, request, operation).await
    }

    // Concept operations (for $lookup and $validate-code)
    async fn get_concept(
        &self,
        code_system_id: &uuid::Uuid,
        code: &str,
    ) -> Result<Option<Concept>, AppError>;

    // Subsumption operations (for $subsumes)
    /// Resolve the selected system, both codes and their relationship atomically.
    /// A missing system returns None; missing codes retain the selected system metadata.
    async fn resolve_subsumption(
        &self,
        url: &str,
        version: Option<&str>,
        code_a: &str,
        code_b: &str,
    ) -> Result<Option<ResolvedSubsumption>, AppError>;
    async fn resolve_subsumption_by_id(
        &self,
        code_system_id: &uuid::Uuid,
        code_a: &str,
        code_b: &str,
    ) -> Result<Option<ResolvedSubsumption>, AppError>;

    // ValueSet expansion operations (for $expand)
    async fn get_value_set_expansion(
        &self,
        value_set_id: &uuid::Uuid,
    ) -> Result<Option<Vec<Value>>, AppError>;
}
