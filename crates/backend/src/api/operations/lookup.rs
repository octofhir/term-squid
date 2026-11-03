use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::{Parameter, Parameters};
use crate::error::AppError;
use crate::store::TerminologyStore;

/// Keep the existing typed header fields while preserving all supported property value types.
#[derive(Serialize)]
pub struct LookupResponse {
    #[serde(rename = "resourceType")]
    resource_type: &'static str,
    parameter: Vec<LookupParameter>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum LookupParameter {
    Standard(Parameter),
    Property(Value),
}

impl From<Parameter> for LookupParameter {
    fn from(parameter: Parameter) -> Self {
        Self::Standard(parameter)
    }
}

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
) -> Result<Json<LookupResponse>, AppError> {
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
) -> Result<Json<LookupResponse>, AppError> {
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
) -> Result<Json<LookupResponse>, AppError> {
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
) -> Result<Json<LookupResponse>, AppError> {
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
) -> Result<Json<LookupResponse>, AppError> {
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
) -> Result<Json<LookupResponse>, AppError> {
    if concept.code.is_none() {
        return Err(if concept.content_mode.as_deref() == Some("complete") {
            AppError::NotFound(format!("Code '{code}' not found"))
        } else {
            AppError::BadRequest("CodeSystem content is incomplete; code cannot be resolved".into())
        });
    }
    // Build result Parameters
    let mut result_params: Vec<LookupParameter> = vec![
        Parameter::string("name", concept.name.as_deref().unwrap_or("")).into(),
        Parameter::string(
            "display",
            concept
                .display
                .as_deref()
                .filter(|display| !display.is_empty())
                .unwrap_or_else(|| concept.code.as_deref().unwrap()),
        )
        .into(),
    ];

    if let Some(version) = &concept.version {
        result_params.push(Parameter::string("version", version).into());
    }
    // Add designation if we have a definition
    if let Some(definition) = &concept.definition {
        result_params.push(
            Parameter::part(
                "designation",
                vec![
                    Parameter::code("use", "definition"),
                    Parameter::string("value", definition.clone()),
                ],
            )
            .into(),
        );
    }

    if let Some(properties) = concept.properties {
        match properties.0 {
            Value::Array(properties) => {
                for property in properties {
                    result_params.push(LookupParameter::Property(typed_property(property)?));
                }
            }
            // Older stored projections use a string-valued map. Preserve that wire contract.
            Value::Object(properties) => {
                for (code, value) in properties {
                    if !valid_code(&code) {
                        return Err(property_error("invalid legacy property code"));
                    }
                    result_params.push(
                        Parameter::part(
                            "property",
                            vec![
                                Parameter::code("code", code),
                                Parameter::string("value", value.to_string()),
                            ],
                        )
                        .into(),
                    );
                }
            }
            _ => return Err(property_error("expected an array or legacy object")),
        }
    }

    Ok(Json(LookupResponse {
        resource_type: "Parameters",
        parameter: result_params,
    }))
}

fn property_error(message: &str) -> AppError {
    AppError::Terminology(term_squid_core::Error(format!(
        "Cannot return stored concept property: {message}"
    )))
}

fn valid_code(value: &str) -> bool {
    let mut whitespace = true;
    for character in value.chars() {
        let next = character.is_whitespace();
        if whitespace && next {
            return false;
        }
        whitespace = next;
    }
    !whitespace
}

fn valid_date_time(value: &str) -> bool {
    let Some(year) = value.get(..4).and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    if year == 0 || !value.as_bytes()[..4].iter().all(u8::is_ascii_digit) {
        return false;
    }
    match value.len() {
        4 => true,
        7 => {
            value.as_bytes()[4] == b'-'
                && value[5..].bytes().all(|b| b.is_ascii_digit())
                && value[5..]
                    .parse::<u32>()
                    .is_ok_and(|month| (1..=12).contains(&month))
        }
        10 => {
            value.as_bytes()[4] == b'-'
                && value.as_bytes()[7] == b'-'
                && value.as_bytes()[5..7].iter().all(u8::is_ascii_digit)
                && value.as_bytes()[8..10].iter().all(u8::is_ascii_digit)
                && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
        }
        _ => {
            value.as_bytes().get(10) == Some(&b'T')
                && chrono::DateTime::parse_from_rfc3339(value).is_ok_and(|date| {
                    date.offset().local_minus_utc().abs() <= 14 * 60 * 60
                        && (value.ends_with('Z')
                            || value
                                .as_bytes()
                                .get(value.len().saturating_sub(6))
                                .is_some_and(|b| matches!(b, b'+' | b'-')))
                })
        }
    }
}

fn valid_coding(value: &Value) -> bool {
    let Some(coding) = value.as_object() else {
        return false;
    };
    !coding.is_empty()
        && coding.iter().all(|(key, value)| match key.as_str() {
            "system" => value
                .as_str()
                .is_some_and(|s| !s.is_empty() && !s.chars().any(char::is_whitespace)),
            "version" | "display" => value.as_str().is_some_and(|s| !s.is_empty()),
            "code" => value.as_str().is_some_and(valid_code),
            "userSelected" => value.is_boolean(),
            _ => false,
        })
}

fn typed_property(property: Value) -> Result<Value, AppError> {
    let Value::Object(mut property) = property else {
        return Err(property_error("property item must be an object"));
    };
    let code = property
        .remove("code")
        .ok_or_else(|| property_error("missing code"))?;
    if !code.as_str().is_some_and(valid_code) {
        return Err(property_error("invalid code"));
    }
    if property.len() != 1 {
        return Err(property_error(
            "expected exactly one supported value[x]; property metadata is unsupported",
        ));
    }
    let (kind, value) = property.into_iter().next().unwrap();
    // These are the seven R4 CodeSystem.concept.property / $lookup value types.
    // Do not coerce or discard unsupported value types and primitive metadata.
    let valid = match kind.as_str() {
        "valueCode" => value.as_str().is_some_and(valid_code),
        "valueCoding" => valid_coding(&value),
        "valueString" => value.as_str().is_some_and(|s| !s.is_empty()),
        "valueInteger" => value.as_i64().is_some_and(|v| i32::try_from(v).is_ok()),
        "valueBoolean" => value.is_boolean(),
        "valueDateTime" => value.as_str().is_some_and(valid_date_time),
        "valueDecimal" => value.is_number(),
        _ => return Err(property_error("unsupported value[x] type")),
    };
    if !valid {
        return Err(property_error(
            "invalid value[x] or unsupported Coding metadata",
        ));
    }
    let mut value_part = serde_json::Map::new();
    value_part.insert("name".into(), "value".into());
    value_part.insert(kind, value);
    Ok(json!({"name":"property","part":[{"name":"code","valueCode":code},value_part]}))
}
