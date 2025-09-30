use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::{Parameter, Parameters};
use crate::error::AppError;
use crate::store::TerminologyStore;

#[derive(Debug, Deserialize)]
pub struct SubsumesParams {
    pub system: Option<String>,
    #[serde(rename = "codeA")]
    pub code_a: Option<String>,
    #[serde(rename = "codeB")]
    pub code_b: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubsumptionOutcome {
    Equivalent,
    Subsumes,
    SubsumedBy,
    NotSubsumed,
}

/// GET /CodeSystem/$subsumes?system=...&codeA=...&codeB=...
pub async fn subsumes_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<SubsumesParams>,
) -> Result<Json<Parameters>, AppError> {
    let system = params
        .system
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;
    let code_a = params
        .code_a
        .ok_or_else(|| AppError::BadRequest("codeA parameter required".to_string()))?;
    let code_b = params
        .code_b
        .ok_or_else(|| AppError::BadRequest("codeB parameter required".to_string()))?;

    perform_subsumes(store, &system, &code_a, &code_b, params.version.as_deref()).await
}

/// POST /CodeSystem/$subsumes with Parameters body
pub async fn subsumes_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let system = params
        .get_string("system")
        .or_else(|| params.get_uri("system"))
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;
    let code_a = params
        .get_string("codeA")
        .or_else(|| params.get_code("codeA"))
        .ok_or_else(|| AppError::BadRequest("codeA parameter required".to_string()))?;
    let code_b = params
        .get_string("codeB")
        .or_else(|| params.get_code("codeB"))
        .ok_or_else(|| AppError::BadRequest("codeB parameter required".to_string()))?;
    let version = params.get_string("version");

    perform_subsumes(store, system, code_a, code_b, version).await
}

/// GET /CodeSystem/{id}/$subsumes?codeA=...&codeB=...
pub async fn subsumes_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<SubsumesParams>,
) -> Result<Json<Parameters>, AppError> {
    let code_a = params
        .code_a
        .ok_or_else(|| AppError::BadRequest("codeA parameter required".to_string()))?;
    let code_b = params
        .code_b
        .ok_or_else(|| AppError::BadRequest("codeB parameter required".to_string()))?;

    let code_system = store
        .get_code_system_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;

    perform_subsumes(store, &code_system.url, &code_a, &code_b, None).await
}

/// POST /CodeSystem/{id}/$subsumes with Parameters body
pub async fn subsumes_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let code_a = params
        .get_string("codeA")
        .or_else(|| params.get_code("codeA"))
        .ok_or_else(|| AppError::BadRequest("codeA parameter required".to_string()))?;
    let code_b = params
        .get_string("codeB")
        .or_else(|| params.get_code("codeB"))
        .ok_or_else(|| AppError::BadRequest("codeB parameter required".to_string()))?;

    let code_system = store
        .get_code_system_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;

    perform_subsumes(store, &code_system.url, code_a, code_b, None).await
}

async fn perform_subsumes(
    store: Arc<dyn TerminologyStore>,
    system: &str,
    code_a: &str,
    code_b: &str,
    version: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    // Get the CodeSystem
    let code_system = store
        .get_code_system(system, version)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem '{system}' not found")))?;

    // Check if both codes exist
    let concept_a = store.get_concept(&code_system.id, code_a).await?;
    let concept_b = store.get_concept(&code_system.id, code_b).await?;

    if concept_a.is_none() {
        return Err(AppError::NotFound(format!(
            "Code '{code_a}' not found in system '{system}'"
        )));
    }

    if concept_b.is_none() {
        return Err(AppError::NotFound(format!(
            "Code '{code_b}' not found in system '{system}'"
        )));
    }

    // Check for equivalence first
    if code_a == code_b {
        return Ok(Json(Parameters::with_parameters(vec![Parameter::code(
            "outcome",
            "equivalent",
        )])));
    }

    // Query closure table for subsumption relationship
    let outcome = store
        .check_subsumption(&code_system.id, code_a, code_b)
        .await?;

    let outcome_code = match outcome {
        Some(true) => "subsumes",     // A subsumes B
        Some(false) => "subsumed-by", // A is subsumed by B
        None => "not-subsumed",       // No relationship
    };

    Ok(Json(Parameters::with_parameters(vec![Parameter::code(
        "outcome",
        outcome_code,
    )])))
}
