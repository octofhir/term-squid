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
    #[serde(rename = "valueSetVersion")]
    pub value_set_version: Option<String>,
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

    perform_expand(store, &url, params, None, None).await
}

/// POST /ValueSet/$expand with Parameters body
pub async fn expand_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Json(params): Json<Parameters>,
) -> Result<Json<serde_json::Value>, AppError> {
    let inline = params.inline_value_set(false)?;
    if inline.is_some() {
        params.validate_inline_fields(&["valueSetVersion", "filter"], &["offset", "count"])?;
    }
    let url = params
        .get_string("url")
        .or_else(|| params.get_uri("url"))
        .or_else(|| inline.map(|_| "urn:term-squid:inline"))
        .ok_or_else(|| AppError::BadRequest("url or valueSet parameter required".to_string()))?;

    let expand_params = ExpandParams {
        url: Some(url.to_string()),
        value_set_version: params.get_string("valueSetVersion").map(str::to_owned),
        filter: params.get_string("filter").map(|s| s.to_string()),
        offset: params.get_integer("offset"),
        count: params.get_integer("count"),
    };

    perform_expand(store, url, expand_params, None, inline).await
}

/// GET /ValueSet/{id}/$expand
pub async fn expand_instance_get(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Query(params): Query<ExpandParams>,
) -> Result<Json<serde_json::Value>, AppError> {
    perform_expand(store, "urn:term-squid:instance", params, Some(id), None).await
}

/// POST /ValueSet/{id}/$expand with Parameters body
pub async fn expand_instance_post(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<Uuid>,
    Json(params): Json<Parameters>,
) -> Result<Json<serde_json::Value>, AppError> {
    params.inline_value_set(true)?;
    let expand_params = ExpandParams {
        url: None,
        value_set_version: params.get_string("valueSetVersion").map(str::to_owned),
        filter: params.get_string("filter").map(str::to_owned),
        offset: params.get_integer("offset"),
        count: params.get_integer("count"),
    };
    perform_expand(
        store,
        "urn:term-squid:instance",
        expand_params,
        Some(id),
        None,
    )
    .await
}

async fn perform_expand(
    store: Arc<dyn TerminologyStore>,
    url: &str,
    params: ExpandParams,
    id: Option<Uuid>,
    inline: Option<&serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    if params.offset.is_some_and(|n| n < 0)
        || params.count.is_some_and(|n| !(0..=10000).contains(&n))
    {
        return Err(AppError::BadRequest(
            "offset must be nonnegative and count must be between 0 and 10000".into(),
        ));
    }
    let expanded = if let Some(id) = id {
        store.expand_compose_by_id(&id).await?
    } else if let Some(resource) = inline {
        store
            .compose_inline(resource, params.value_set_version.as_deref(), None)
            .await?
    } else {
        store
            .expand_compose(url, params.value_set_version.as_deref())
            .await?
    };
    let filter = params.filter.as_ref().map(|s| s.to_lowercase());
    let offset = params.offset.unwrap_or(0) as usize;
    let count = params.count.unwrap_or(100) as usize;
    let mut total = 0;
    let mut paginated_entries = Vec::new();
    for entry in expanded.members.values().filter(|entry| {
        filter.as_ref().is_none_or(|f| {
            entry.code.to_lowercase().contains(f)
                || entry
                    .display
                    .as_ref()
                    .is_some_and(|d| d.to_lowercase().contains(f))
        })
    }) {
        if total >= offset && paginated_entries.len() < count {
            paginated_entries.push(entry);
        }
        total += 1;
    }

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
    let mut result = expanded.resource.as_ref().clone();
    if let Some(obj) = result.as_object_mut() {
        obj.insert("expansion".to_string(), expansion);
    }

    Ok(Json(result))
}
