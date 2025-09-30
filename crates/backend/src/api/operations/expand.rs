use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::parameters::Parameters;
use crate::error::AppError;
use crate::store::TerminologyStore;

#[derive(Debug, Deserialize)]
pub struct ExpandParams {
    pub url: Option<String>,
    pub filter: Option<String>,
    pub offset: Option<i64>,
    pub count: Option<i64>,
}

/// GET /ValueSet/$expand?url=...
pub async fn expand_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(params): Query<ExpandParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let url = params
        .url
        .clone()
        .ok_or_else(|| AppError::BadRequest("url parameter required".to_string()))?;

    perform_expand(store, &url, params).await
}

/// POST /ValueSet/$expand with Parameters body
pub async fn expand_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Json(params): Json<Parameters>,
) -> Result<Json<serde_json::Value>, AppError> {
    let url = params
        .get_string("url")
        .or_else(|| params.get_uri("url"))
        .ok_or_else(|| AppError::BadRequest("url parameter required".to_string()))?;

    let expand_params = ExpandParams {
        url: Some(url.to_string()),
        filter: params.get_string("filter").map(|s| s.to_string()),
        offset: None,
        count: None,
    };

    perform_expand(store, url, expand_params).await
}

/// GET /ValueSet/{id}/$expand
pub async fn expand_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<ExpandParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    let value_set = store
        .get_value_set_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ValueSet {id} not found")))?;

    perform_expand(store, &value_set.url, params).await
}

/// POST /ValueSet/{id}/$expand with Parameters body
pub async fn expand_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Json(params): Json<Parameters>,
) -> Result<Json<serde_json::Value>, AppError> {
    let value_set = store
        .get_value_set_by_id(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ValueSet {id} not found")))?;

    let expand_params = ExpandParams {
        url: Some(value_set.url.clone()),
        filter: params.get_string("filter").map(|s| s.to_string()),
        offset: None,
        count: None,
    };

    perform_expand(store, &value_set.url, expand_params).await
}

async fn perform_expand(
    store: Arc<dyn TerminologyStore>,
    url: &str,
    params: ExpandParams,
) -> Result<Json<serde_json::Value>, AppError> {
    // Get the ValueSet
    let value_set = store
        .get_value_set(url, None)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("ValueSet '{url}' not found")))?;

    // Get the expansion from the database
    let mut expansion_entries = store
        .get_value_set_expansion(&value_set.id)
        .await?
        .unwrap_or_default();

    // Apply filter if provided
    if let Some(filter_text) = &params.filter {
        let filter_lower = filter_text.to_lowercase();
        expansion_entries.retain(|entry| {
            if let Some(display) = entry.get("display").and_then(|v| v.as_str()) {
                display.to_lowercase().contains(&filter_lower)
            } else if let Some(code) = entry.get("code").and_then(|v| v.as_str()) {
                code.to_lowercase().contains(&filter_lower)
            } else {
                false
            }
        });
    }

    let total = expansion_entries.len();

    // Apply pagination
    let offset = params.offset.unwrap_or(0) as usize;
    let count = params.count.unwrap_or(100) as usize;

    let paginated_entries: Vec<_> = expansion_entries
        .into_iter()
        .skip(offset)
        .take(count)
        .collect();

    // Build ValueSet with expansion
    let expansion = json!({
        "identifier": format!("urn:uuid:{}", Uuid::new_v4()),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "total": total,
        "offset": offset,
        "parameter": [],
        "contains": paginated_entries
    });

    // Extract the base ValueSet content and add expansion
    let mut result = value_set.content.0.clone();
    if let Some(obj) = result.as_object_mut() {
        obj.insert("expansion".to_string(), expansion);
    }

    Ok(Json(result))
}
