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
use crate::models::{SearchParams, ValueSet};
use crate::store::TerminologyStore;

pub fn valueset_routes() -> Router<Arc<dyn TerminologyStore>> {
    Router::new()
        .route("/ValueSet", get(search_valuesets))
        .route("/ValueSet/{id}", get(get_valueset))
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

async fn get_valueset(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<String>,
) -> Result<Json<ValueSet>, AppError> {
    let value_set = if let Ok(uuid) = Uuid::parse_str(&id) {
        store.get_value_set_by_id(&uuid).await?
    } else {
        store.get_value_set(&id, None).await?
    };

    value_set
        .ok_or_else(|| AppError::NotFound("ValueSet not found".to_string()))
        .map(Json)
}

async fn search_valuesets(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, AppError> {
    // Get total count first
    let total = store.count_value_sets().await?;

    let params: SearchParams = query.into();
    let results = store.search_value_sets(&params).await?;

    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "searchset",
        "total": total,
        "entry": results.iter().map(|vs| {
            serde_json::json!({
                "resource": vs.content.0,
                "search": {
                    "mode": "match"
                }
            })
        }).collect::<Vec<_>>()
    });

    Ok(Json(bundle))
}
