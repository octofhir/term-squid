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
    #[serde(rename = "targetsystem")]
    pub target_system: Option<String>,
    pub target: Option<String>,
    #[serde(rename = "conceptMapVersion")]
    pub map_version: Option<String>,
    pub reverse: Option<bool>,
}

/// GET /ConceptMap/$translate?code=...&system=...&target=...
pub async fn translate_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<TranslateParams>,
) -> Result<Json<Parameters>, AppError> {
    if params.target.is_some() {
        return Err(AppError::BadRequest(
            "target ValueSet constraints are not supported; use targetsystem for a system".into(),
        ));
    }
    let code = params
        .code
        .ok_or_else(|| AppError::BadRequest("code parameter required".to_string()))?;
    let system = params
        .system
        .ok_or_else(|| AppError::BadRequest("system parameter required".to_string()))?;

    perform_translate(
        store,
        params.url.as_deref(),
        params.map_version.as_deref(),
        &system,
        &code,
        params.target_system.as_deref(),
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
    if params.get_string("target").is_some() {
        return Err(AppError::BadRequest(
            "target ValueSet constraints are not supported".into(),
        ));
    }
    let target = params.get_string("targetsystem");
    let reverse = params.get_boolean("reverse").unwrap_or(false);

    perform_translate(
        store,
        url,
        params.get_string("conceptMapVersion"),
        system,
        code,
        target,
        reverse,
    )
    .await
}

/// GET /ConceptMap/{id}/$translate?code=...&system=...
pub async fn translate_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<TranslateParams>,
) -> Result<Json<Parameters>, AppError> {
    if params.target.is_some() {
        return Err(AppError::BadRequest(
            "target ValueSet constraints are not supported; use targetsystem for a system".into(),
        ));
    }
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

    translate_map(
        &concept_map.content.0,
        &system,
        &code,
        params.target_system.as_deref(),
        params.reverse.unwrap_or(false),
    )
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
    if params.get_string("target").is_some() {
        return Err(AppError::BadRequest(
            "target ValueSet constraints are not supported".into(),
        ));
    }
    let target = params.get_string("targetsystem");
    let reverse = params.get_boolean("reverse").unwrap_or(false);

    let concept_map = store
        .get_concept_map_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ConceptMap {id} not found")))?;

    translate_map(&concept_map.content.0, system, code, target, reverse)
}

async fn perform_translate(
    store: Arc<dyn TerminologyStore>,
    concept_map_url: Option<&str>,
    map_version: Option<&str>,
    source_system: &str,
    source_code: &str,
    target_system: Option<&str>,
    reverse: bool,
) -> Result<Json<Parameters>, AppError> {
    let url = concept_map_url.ok_or_else(|| {
        AppError::BadRequest("ConceptMap URL is required; map search is not supported".into())
    })?;
    let map = store
        .get_concept_map(url, map_version)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ConceptMap '{url}' not found")))?;
    translate_map(
        &map.content.0,
        source_system,
        source_code,
        target_system,
        reverse,
    )
}

fn translate_map(
    content: &serde_json::Value,
    source_system: &str,
    source_code: &str,
    target_system: Option<&str>,
    reverse: bool,
) -> Result<Json<Parameters>, AppError> {
    let mut matches = Vec::new();

    {
        if let Some(groups) = content.get("group").and_then(|g| g.as_array()) {
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

                            if reverse || element_code == Some(source_code) {
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

                                        if target.get("dependsOn").is_some()
                                            || target.get("product").is_some()
                                        {
                                            return Err(AppError::BadRequest(
                                                "Conditional mappings are not supported".into(),
                                            ));
                                        }
                                        if matches!(equivalence, "unmatched" | "disjoint") {
                                            continue;
                                        }
                                        if reverse && target_code != Some(source_code) {
                                            continue;
                                        }
                                        let mapped_code =
                                            if reverse { element_code } else { target_code };
                                        let mapped_display = if reverse {
                                            element.get("display").and_then(|d| d.as_str())
                                        } else {
                                            target_display
                                        };
                                        let equivalence = if reverse {
                                            match equivalence {
                                                "wider" => "narrower",
                                                "narrower" => "wider",
                                                "subsumes" => "specializes",
                                                "specializes" => "subsumes",
                                                e => e,
                                            }
                                        } else {
                                            equivalence
                                        };
                                        if let Some(target_code) = mapped_code {
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
                                            if let Some(display) = mapped_display {
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
            format!("No translation found for code '{source_code}' in system '{source_system}'"),
        ));
    } else {
        result_params.extend(matches);
    }

    Ok(Json(Parameters::with_parameters(result_params)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reverse_uses_target_code_and_source_equivalence() {
        let map = json!({"group":[{"source":"urn:source","target":"urn:target","element":[
            {"code":"a","display":"Source A","target":[{"code":"b","display":"Target B","equivalence":"wider"}]}]}]});
        let response = translate_map(&map, "urn:target", "b", Some("urn:source"), true)
            .unwrap()
            .0;
        let value = serde_json::to_value(response).unwrap();
        let parts = value["parameter"][1]["part"].as_array().unwrap();
        assert!(parts.iter().any(|p| p["name"] == "concept"
            && p["valueCoding"]["code"] == "a"
            && p["valueCoding"]["display"] == "Source A"));
        assert!(parts
            .iter()
            .any(|p| p["name"] == "equivalence" && p["valueCode"] == "narrower"));
        assert_eq!(
            translate_map(&map, "urn:target", "a", None, true)
                .unwrap()
                .0
                .get_boolean("result"),
            Some(false)
        );
        assert_eq!(
            translate_map(&map, "urn:source", "a", Some("urn:wrong"), false)
                .unwrap()
                .0
                .get_boolean("result"),
            Some(false)
        );
    }

    #[test]
    fn disjoint_and_conditional_mappings_are_not_positive_matches() {
        let map = json!({"group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"disjoint"}]}]}]});
        assert_eq!(
            translate_map(&map, "urn:s", "a", None, false)
                .unwrap()
                .0
                .get_boolean("result"),
            Some(false)
        );
        let map = json!({"group":[{"source":"urn:s","target":"urn:t","element":[{"code":"a","target":[{"code":"b","equivalence":"equivalent","dependsOn":[{}]}]}]}]});
        assert!(translate_map(&map, "urn:s", "a", None, false).is_err());
    }
}
