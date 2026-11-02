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

    let concept = store
        .resolve_concept_by_id(&id, &code)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;
    lookup_result(concept, &code)
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

    let concept = store
        .resolve_concept_by_id(&id, code)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;
    lookup_result(concept, code)
}

async fn perform_lookup(
    store: Arc<dyn TerminologyStore>,
    system: &str,
    code: &str,
    version: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    let concept = store
        .resolve_concept(system, version, code)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "CodeSystem '{system}' version {version:?} not found"
            ))
        })?;
    lookup_result(concept, code)
}

fn lookup_result(
    concept: crate::models::ResolvedConcept,
    code: &str,
) -> Result<Json<Parameters>, AppError> {
    if concept.code.is_none() {
        return Err(if concept.content_mode.as_deref() == Some("complete") {
            AppError::NotFound(format!("Code '{code}' not found"))
        } else {
            AppError::BadRequest("CodeSystem content is incomplete; code cannot be resolved".into())
        });
    }
    // Build result Parameters
    let mut result_params = vec![
        Parameter::string("name", concept.name.as_deref().unwrap_or("")),
        Parameter::string("display", concept.display.as_deref().unwrap_or("")),
    ];

    if let Some(version) = &concept.version {
        result_params.push(Parameter::string("version", version));
    }
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
