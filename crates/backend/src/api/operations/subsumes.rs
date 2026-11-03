use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
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

    perform_subsumes(
        store,
        &system,
        &code_a,
        &code_b,
        params.version.as_deref(),
        None,
    )
    .await
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

    perform_subsumes(store, system, code_a, code_b, version, None).await
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

    perform_subsumes(store, "", &code_a, &code_b, None, Some(id)).await
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

    perform_subsumes(store, "", code_a, code_b, None, Some(id)).await
}

async fn perform_subsumes(
    store: Arc<dyn TerminologyStore>,
    system: &str,
    code_a: &str,
    code_b: &str,
    version: Option<&str>,
    id: Option<Uuid>,
) -> Result<Json<Parameters>, AppError> {
    let resolved = if let Some(id) = id {
        store.resolve_subsumption_by_id(&id, code_a, code_b).await?
    } else {
        store
            .resolve_subsumption(system, version, code_a, code_b)
            .await?
    }
    .ok_or_else(|| AppError::NotFound(format!("CodeSystem '{system}' not found")))?;
    if resolved.hierarchy_meaning.as_deref() != Some("is-a") {
        return Err(AppError::BadRequest(
            "Subsumption requires an is-a hierarchy".into(),
        ));
    }
    if resolved.code_a.is_none() || resolved.code_b.is_none() {
        return Err(AppError::NotFound("Subsumption code not found".into()));
    }
    let outcome_code = if resolved.code_a == resolved.code_b {
        "equivalent"
    } else {
        match resolved.relationship {
            Some(true) => "subsumes",
            Some(false) => "subsumed-by",
            None => "not-subsumed",
        }
    };

    Ok(Json(Parameters::with_parameters(vec![Parameter::code(
        "outcome",
        outcome_code,
    )])))
}
