use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::{CodeSystem, SearchParams};
use crate::store::TerminologyStore;

pub fn codesystem_routes() -> Router<Arc<dyn TerminologyStore>> {
    Router::new()
        .route("/CodeSystem", get(search_codesystems))
        .route("/CodeSystem/{id}", get(get_codesystem))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    url: Option<String>,
    name: Option<String>,
    status: Option<String>,
    #[serde(rename = "fhirVersion")]
    fhir_version: Option<String>,
    #[serde(rename = "_count")]
    count: Option<i64>,
    #[serde(rename = "_offset")]
    offset: Option<i64>,
}

impl From<SearchQuery> for SearchParams {
    fn from(query: SearchQuery) -> Self {
        SearchParams {
            url: query.url,
            name: query.name,
            status: query.status,
            fhir_version: query.fhir_version,
            limit: query.count,
            offset: query.offset,
        }
    }
}

async fn get_codesystem(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<String>,
) -> Result<Json<CodeSystem>, AppError> {
    // Try to parse as UUID first, otherwise treat as URL
    let code_system = if let Ok(uuid) = Uuid::parse_str(&id) {
        store.get_code_system_by_id(&uuid).await?
    } else {
        store.get_code_system(&id, None).await?
    };

    code_system
        .ok_or_else(|| AppError::NotFound("CodeSystem not found".to_string()))
        .map(Json)
}

async fn search_codesystems(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, AppError> {
    // Get total count first
    let total = store.count_code_systems().await?;

    let params: SearchParams = query.into();
    let results = store.search_code_systems(&params).await?;

    // Create FHIR Bundle
    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "searchset",
        "total": total,
        "entry": results.iter().map(|cs| {
            serde_json::json!({
                "resource": cs.content.0,
                "search": {
                    "mode": "match"
                }
            })
        }).collect::<Vec<_>>()
    });

    Ok(Json(bundle))
}
