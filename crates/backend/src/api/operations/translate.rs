use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::{Coding, Parameter, Parameters};
use crate::error::AppError;
use crate::store::TerminologyStore;

#[derive(Debug, Deserialize)]
pub struct TranslateParams {
    pub url: Option<String>,
    pub code: Option<String>,
    pub system: Option<String>,
    pub target: Option<String>,
    pub reverse: Option<bool>,
}

/// GET /ConceptMap/$translate?code=...&system=...&target=...
pub async fn translate_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<TranslateParams>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params
        .system
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;

    perform_translate(
        store,
        params.url.as_deref(),
        &system,
        &code,
        params.target.as_deref(),
        params.reverse.unwrap_or(false),
    )
    .await
}

/// POST /ConceptMap/$translate with Parameters body
pub async fn translate_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params
        .get_string("system")
        .or_else(|| params.get_uri("system"))
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;
    let url = params.get_string("url").or_else(|| params.get_uri("url"));
    let target = params.get_string("target");
    let reverse = params.get_boolean("reverse").unwrap_or(false);

    perform_translate(store, url, system, code, target, reverse).await
}

/// GET /ConceptMap/{id}/$translate?code=...&system=...
pub async fn translate_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<TranslateParams>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params
        .system
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;

    let concept_map = store
        .get_concept_map_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ConceptMap {id} not found")))?;

    perform_translate(
        store,
        Some(&concept_map.url),
        &system,
        &code,
        params.target.as_deref(),
        params.reverse.unwrap_or(false),
    )
    .await
}

/// POST /ConceptMap/{id}/$translate with Parameters body
pub async fn translate_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Json(params): Json<Parameters>,
) -> Result<Json<Parameters>, AppError> {
    let code = params
        .get_string("code")
        .or_else(|| params.get_code("code"))
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params
        .get_string("system")
        .or_else(|| params.get_uri("system"))
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;
    let target = params.get_string("target");
    let reverse = params.get_boolean("reverse").unwrap_or(false);

    let concept_map = store
        .get_concept_map_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ConceptMap {id} not found")))?;

    perform_translate(store, Some(&concept_map.url), system, code, target, reverse).await
}

async fn perform_translate(
    store: Arc<dyn TerminologyStore>,
    concept_map_url: Option<&str>,
    source_system: &str,
    source_code: &str,
    target_system: Option<&str>,
    reverse: bool,
) -> Result<Json<Parameters>, AppError> {
    // Get ConceptMaps that can translate from this system
    let concept_maps = if let Some(url) = concept_map_url {
        // Use specific ConceptMap
        vec![store
            .get_concept_map(url, None)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("ConceptMap '{url}' not found")))?]
    } else {
        // Find all ConceptMaps for this source/target pair
        // TODO: Implement search for ConceptMaps by source/target
        return Ok(Json(Parameters::with_parameters(vec![
            Parameter::boolean("result", false),
            Parameter::string(
                "message",
                "ConceptMap URL parameter is required (search not yet implemented)",
            ),
        ])));
    };

    // For now, return a placeholder response
    // Full implementation requires parsing ConceptMap.group.element.target
    // and performing the translation lookup

    let mut matches = Vec::new();

    for concept_map in &concept_maps {
        // Parse the ConceptMap JSON to find translations
        if let Some(groups) = concept_map.content.get("group").and_then(|g| g.as_array()) {
            for group in groups {
                let group_source = group.get("source").and_then(|s| s.as_str());
                let group_target = group.get("target").and_then(|t| t.as_str());

                // Check if this group matches our source system
                let matches_source = if reverse {
                    group_target == Some(source_system)
                } else {
                    group_source == Some(source_system)
                };

                if matches_source {
                    // Look through elements for our code
                    if let Some(elements) = group.get("element").and_then(|e| e.as_array()) {
                        for element in elements {
                            let element_code = element.get("code").and_then(|c| c.as_str());

                            if element_code == Some(source_code) {
                                // Found a match, extract targets
                                if let Some(targets) =
                                    element.get("target").and_then(|t| t.as_array())
                                {
                                    for target in targets {
                                        let target_code =
                                            target.get("code").and_then(|c| c.as_str());
                                        let target_display =
                                            target.get("display").and_then(|d| d.as_str());
                                        let equivalence = target
                                            .get("equivalence")
                                            .and_then(|e| e.as_str())
                                            .unwrap_or("equivalent");

                                        if let Some(target_code) = target_code {
                                            let target_system_str = if reverse {
                                                group_source.unwrap_or("")
                                            } else {
                                                group_target.unwrap_or("")
                                            };

                                            // Filter by target system if specified
                                            if let Some(ts) = target_system {
                                                if target_system_str != ts {
                                                    continue;
                                                }
                                            }

                                            let mut coding =
                                                Coding::new(target_system_str, target_code);
                                            if let Some(display) = target_display {
                                                coding = coding.with_display(display);
                                            }

                                            matches.push(Parameter::part(
                                                "match",
                                                vec![
                                                    Parameter::code("equivalence", equivalence),
                                                    Parameter::coding("concept", coding),
                                                ],
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut result_params = vec![Parameter::boolean("result", !matches.is_empty())];

    if matches.is_empty() {
        result_params.push(Parameter::string(
            "message",
            format!(
                "No translation found for code '{source_code}' in system '{source_system}'"
            ),
        ));
    } else {
        result_params.extend(matches);
    }

    Ok(Json(Parameters::with_parameters(result_params)))
}
