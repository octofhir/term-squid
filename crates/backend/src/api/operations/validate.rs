use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::{Parameter, Parameters};
use crate::error::AppError;
use crate::store::{TerminologyStore, ValueSetOperation, ValueSetRequest, ValueSetResult};

#[derive(Debug, Deserialize)]
pub struct ValidateCodeParams {
    pub url: Option<String>,
    pub system: Option<String>,
    pub code: Option<String>,
    pub version: Option<String>,
    pub display: Option<String>,
    #[serde(rename = "valueSetVersion")]
    pub value_set_version: Option<String>,
    #[serde(rename = "systemVersion")]
    pub system_version: Option<String>,
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
        .or_else(|| params.get_uri("system"))
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

    let resolved = store
        .resolve_concept_by_id(&id, &code)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;
    validate_result(resolved, &code, params.display.as_deref())
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

    let resolved = store
        .resolve_concept_by_id(&id, code)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("CodeSystem {id} not found")))?;
    validate_result(resolved, code, display)
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
        params.value_set_version.as_deref(),
        params.system_version.as_deref(),
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
    let inline = params.inline_value_set(false)?;
    if inline.is_some() {
        params.validate_inline_fields(
            &[
                "valueSetVersion",
                "system",
                "code",
                "systemVersion",
                "display",
            ],
            &[],
        )?;
    }
    let value_set_url = params
        .get_string("url")
        .or_else(|| params.get_uri("url"))
        .or_else(|| inline.map(|_| "urn:term-squid:inline"))
        .ok_or_else(|| {
            AppError::BadRequest("url or valueSet parameter required for ValueSet".to_string())
        })?;
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params.get_string("system").ok_or_else(|| {
        AppError::BadRequest("system parameter required for ValueSet validation".to_string())
    })?;
    let display = params.get_string("display");

    if let Some(resource) = inline {
        let system_version = params.get_string("systemVersion");
        return validate_value_set(
            &*store,
            ValueSetRequest {
                url: value_set_url,
                version: params.get_string("valueSetVersion"),
                id: None,
                inline: Some(resource),
                system_context: system_version.map(|v| (system, v)),
            },
            system_version,
            system,
            code,
            display,
        )
        .await;
    }

    perform_validate_code_valueset(
        store,
        value_set_url,
        params.get_string("valueSetVersion"),
        params.get_string("systemVersion"),
        system,
        code,
        display,
    )
    .await
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

    validate_value_set(
        &*store,
        ValueSetRequest {
            url: "urn:term-squid:instance",
            version: None,
            id: Some(id),
            inline: None,
            system_context: params
                .system_version
                .as_deref()
                .map(|v| (system.as_str(), v)),
        },
        params.system_version.as_deref(),
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
    params.inline_value_set(true)?;
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params.get_string("system").ok_or_else(|| {
        AppError::BadRequest("system parameter required for ValueSet validation".to_string())
    })?;
    let display = params.get_string("display");

    let version = params.get_string("systemVersion");
    validate_value_set(
        &*store,
        ValueSetRequest {
            url: "urn:term-squid:instance",
            version: None,
            id: Some(id),
            inline: None,
            system_context: version.map(|v| (system, v)),
        },
        version,
        system,
        code,
        display,
    )
    .await
}

async fn perform_validate_code(
    store: Arc<dyn TerminologyStore>,
    system: &str,
    code: &str,
    version: Option<&str>,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    let resolved = store
        .resolve_concept(system, version, code)
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "CodeSystem '{system}' version {version:?} not found"
            ))
        })?;
    validate_result(resolved, code, display)
}

fn validate_result(
    resolved: crate::models::ResolvedConcept,
    code: &str,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    if resolved.code.is_none() && resolved.content_mode.as_deref() != Some("complete") {
        return Err(AppError::BadRequest(
            "CodeSystem content is incomplete; membership is unknown".into(),
        ));
    }
    let mut valid = resolved.code.is_some();
    let mut message = if valid {
        None
    } else {
        Some(format!("Code '{code}' not found"))
    };
    if let Some(expected) = display {
        if resolved.display.as_deref() != Some(expected) && valid {
            valid = false;
            message = Some(format!(
                "Display '{expected}' does not match {:?}",
                resolved.display
            ));
        }
    }
    let mut result = vec![Parameter::boolean("result", valid)];
    if let Some(display) = resolved.display {
        result.push(Parameter::string("display", display));
    }
    if let Some(version) = resolved.version {
        result.push(Parameter::string("version", version));
    }
    if let Some(message) = message {
        result.push(Parameter::string("message", message));
    }
    Ok(Json(Parameters::with_parameters(result)))
}

async fn perform_validate_code_valueset(
    store: Arc<dyn TerminologyStore>,
    value_set_url: &str,
    value_set_version: Option<&str>,
    system_version: Option<&str>,
    system: &str,
    code: &str,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    validate_value_set(
        &*store,
        ValueSetRequest {
            url: value_set_url,
            version: value_set_version,
            id: None,
            inline: None,
            system_context: system_version.map(|v| (system, v)),
        },
        system_version,
        system,
        code,
        display,
    )
    .await
}

async fn validate_value_set(
    store: &dyn TerminologyStore,
    request: ValueSetRequest<'_>,
    system_version: Option<&str>,
    system: &str,
    code: &str,
    display: Option<&str>,
) -> Result<Json<Parameters>, AppError> {
    let result = store
        .query_value_set(
            request,
            ValueSetOperation::Find {
                system,
                version: system_version,
                code,
            },
        )
        .await?;
    let member = match &result {
        ValueSetResult::Native(page) => page.members.first(),
        ValueSetResult::Finite(plan) => plan.members.find(system, system_version, code),
    };
    let valid = member.is_some_and(|m| display.is_none_or(|d| m.display.as_deref() == Some(d)));
    let mut result = vec![Parameter::boolean("result", valid)];
    if let Some(m) = member {
        if let Some(display) = &m.display {
            result.push(Parameter::string("display", display));
        }
        if let Some(version) = &m.version {
            result.push(Parameter::string("version", version));
        }
    }
    if !valid {
        result.push(Parameter::string(
            "message",
            "Code, version or display is not in the ValueSet",
        ));
    }
    Ok(Json(Parameters::with_parameters(result)))
}
