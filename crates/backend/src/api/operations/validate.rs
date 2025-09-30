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
pub struct ValidateCodeParams {
    pub url: Option<String>,
    pub system: Option<String>,
    pub code: Option<String>,
    pub version: Option<String>,
    pub display: Option<String>,
}

/// GET /CodeSystem/$validate-code?url=...&code=...
pub async fn validate_code_cs_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    let system = params
        .system
        .or(params.url)
        .ok_or_else(|| AppError::BadRequest("system or url parameter required".to_string()))?;
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;

    perform_validate_code(
        store,
        &system,
        &code,
        params.version.as_deref(),
        params.display.as_deref(),
    )
    .await
}

/// POST /CodeSystem/$validate-code with Parameters body
pub async fn validate_code_cs_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let system = params
        .get_string("system")
        .or_else(|| params.get_string("url"))
        .or_else(|| params.get_uri("url"))
        .ok_or_else(|| AppError::BadRequest("system or url parameter required".to_string()))?;
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let version = params.get_string("version");
    let display = params.get_string("display");

    perform_validate_code(store, system, code, version, display).await
}

/// GET /CodeSystem/{id}/$validate-code?code=...
pub async fn validate_code_cs_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;

    let code_system = store
        .get_code_system_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;

    perform_validate_code(
        store,
        &code_system.url,
        &code,
        None,
        params.display.as_deref(),
    )
    .await
}

/// POST /CodeSystem/{id}/$validate-code with Parameters body
pub async fn validate_code_cs_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let display = params.get_string("display");

    let code_system = store
        .get_code_system_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;

    perform_validate_code(store, &code_system.url, code, None, display).await
}

/// GET /ValueSet/$validate-code?url=...&code=...&system=...
pub async fn validate_code_vs_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    let value_set_url = params
        .url
        .ok_or_else(|| AppError::BadRequest("url parameter required for ValueSet".to_string()))?;
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params.system.ok_or_else(|| {
        AppError::BadRequest("system parameter required for ValueSet validation".to_string())
    })?;

    perform_validate_code_valueset(
        store,
        &value_set_url,
        &system,
        &code,
        params.display.as_deref(),
    )
    .await
}

/// POST /ValueSet/$validate-code with Parameters body
pub async fn validate_code_vs_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let value_set_url = params
        .get_string("url")
        .or_else(|| params.get_uri("url"))
        .ok_or_else(|| AppError::BadRequest("url parameter required for ValueSet".to_string()))?;
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params.get_string("system").ok_or_else(|| {
        AppError::BadRequest("system parameter required for ValueSet validation".to_string())
    })?;
    let display = params.get_string("display");

    perform_validate_code_valueset(store, value_set_url, system, code, display).await
}

/// GET /ValueSet/{id}/$validate-code?code=...&system=...
pub async fn validate_code_vs_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<ValidateCodeParams>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params.system.ok_or_else(|| {
        AppError::BadRequest("system parameter required for ValueSet validation".to_string())
    })?;

    let value_set = store
        .get_value_set_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ValueSet {id} not found")))?;

    perform_validate_code_valueset(
        store,
        &value_set.url,
        &system,
        &code,
        params.display.as_deref(),
    )
    .await
}

/// POST /ValueSet/{id}/$validate-code with Parameters body
pub async fn validate_code_vs_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params.get_string("system").ok_or_else(|| {
        AppError::BadRequest("system parameter required for ValueSet validation".to_string())
    })?;
    let display = params.get_string("display");

    let value_set = store
        .get_value_set_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ValueSet {id} not found")))?;

    perform_validate_code_valueset(store, &value_set.url, system, code, display).await
}

async fn perform_validate_code(
    store: Arc<dyn TerminologyStore>,
    system: &str,
    code: &str,
    version: Option<&str>,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    // Check if CodeSystem exists
    let code_system = store.get_code_system(system, version).await?;
    if code_system.is_none() {
        return Ok(Json(Parameters::with_parameters(vec![
            Parameter::boolean("result", false),
            Parameter::string("message", format!("CodeSystem '{system}' not found")),
        ])));
    }

    let code_system = code_system.unwrap();

    // Check if code exists in the system
    let concept = store.get_concept(&code_system.id, code).await?;

    let is_valid = concept.is_some();
    let mut result_params = vec![Parameter::boolean("result", is_valid)];

    if is_valid {
        let concept = concept.unwrap();

        // Optionally validate display
        if let Some(expected_display) = display {
            if let Some(actual_display) = &concept.display {
                if actual_display != expected_display {
                    result_params.push(Parameter::string(
                        "message",
                        format!(
                            "Display value '{expected_display}' does not match expected '{actual_display}'"
                        ),
                    ));
                }
            }
        }

        result_params.push(Parameter::string(
            "display",
            concept.display.unwrap_or_default(),
        ));
    } else {
        result_params.push(Parameter::string(
            "message",
            format!("Code '{code}' not found in system '{system}'"),
        ));
    }

    Ok(Json(Parameters::with_parameters(result_params)))
}

async fn perform_validate_code_valueset(
    store: Arc<dyn TerminologyStore>,
    _value_set_url: &str,
    system: &str,
    code: &str,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    // First validate the code exists in the specified system
    let code_validation = perform_validate_code(store.clone(), system, code, None, display).await?;

    let code_valid = code_validation.0.get_boolean("result").unwrap_or(false);

    if !code_valid {
        return Ok(code_validation);
    }

    // TODO: Check if the ValueSet includes this code from this system
    // For now, we'll just validate the code exists in the system
    // Full implementation requires expanding the ValueSet and checking membership

    Ok(Json(Parameters::with_parameters(vec![
        Parameter::boolean("result", true),
        Parameter::string(
            "message",
            "Code validation passed (ValueSet expansion not yet implemented)",
        ),
    ])))
}
