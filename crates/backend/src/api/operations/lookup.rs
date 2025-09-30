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
pub struct LookupParams {
    pub system: Option<String>,
    pub code: Option<String>,
    pub version: Option<String>,
}

/// GET /CodeSystem/$lookup?system=...&code=...
pub async fn lookup_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<LookupParams>,
) -> Result<Json<Parameters>, AppError> {
    let system = params
        .system
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;

    perform_lookup(store, &system, &code, params.version.as_deref()).await
}

/// POST /CodeSystem/$lookup with Parameters body
pub async fn lookup_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let system = params
        .get_string("system")
        .or_else(|| params.get_uri("system"))
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let version = params.get_string("version");

    perform_lookup(store, system, code, version).await
}

/// GET /CodeSystem/{id}/$lookup?code=...
pub async fn lookup_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<LookupParams>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;

    // Get CodeSystem by ID to extract system URL
    let code_system = store
        .get_code_system_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;

    perform_lookup(store, &code_system.url, &code, None).await
}

/// POST /CodeSystem/{id}/$lookup with Parameters body
pub async fn lookup_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;

    // Get CodeSystem by ID to extract system URL
    let code_system = store
        .get_code_system_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;

    perform_lookup(store, &code_system.url, code, None).await
}

async fn perform_lookup(
    store: Arc<dyn TerminologyStore>,
    system: &str,
    code: &str,
    version: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    // Get the CodeSystem
    let code_system = store
        .get_code_system(system, version)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem '{system}' not found")))?;

    // Find the concept in the database
    let concept = store
        .get_concept(&code_system.id, code)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!("Code '{code}' not found in system '{system}'"))
        })?;

    // Build result Parameters
    let mut result_params = vec![
        Parameter::string("name", code_system.name.as_deref().unwrap_or("")),
        Parameter::string("display", concept.display.as_deref().unwrap_or("")),
    ];

    // Add designation if we have a definition
    if let Some(definition) = &concept.definition {
        result_params.push(Parameter::part(
            "designation",
            vec![
                Parameter::code("use", "definition"),
                Parameter::string("value", definition.clone()),
            ],
        ));
    }

    // Add properties if present in the concept
    if let Some(properties) = &concept.properties {
        if let Some(props_obj) = properties.as_object() {
            for (key, value) in props_obj {
                result_params.push(Parameter::part(
                    "property",
                    vec![
                        Parameter::code("code", key.clone()),
                        Parameter::string("value", value.to_string()),
                    ],
                ));
            }
        }
    }

    Ok(Json(Parameters::with_parameters(result_params)))
}
