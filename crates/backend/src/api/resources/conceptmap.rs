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
use crate::models::{ConceptMap, SearchParams};
use crate::store::TerminologyStore;

pub fn conceptmap_routes() -> Router<Arc<dyn TerminologyStore>> {
    Router::new()
        .route("/ConceptMap", get(search_conceptmaps))
        .route("/ConceptMap/{id}", get(get_conceptmap))
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    url: Option<String>,
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
            name: None,
            status: query.status,
            fhir_version: query.fhir_version,
            limit: query.count,
            offset: query.offset,
        }
    }
}

async fn get_conceptmap(
    State(store): State<Arc<dyn TerminologyStore>>,
    Path(id): Path<String>,
) -> Result<Json<ConceptMap>, AppError> {
    let concept_map = if let Ok(uuid) = Uuid::parse_str(&id) {
        store.get_concept_map_by_id(&uuid).await?
    } else {
        store.get_concept_map(&id, None).await?
    };

    concept_map
        .ok_or_else(|| AppError::NotFound("ConceptMap not found".to_string()))
        .map(Json)
}

async fn search_conceptmaps(
    State(store): State<Arc<dyn TerminologyStore>>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, AppError> {
    // Get total count first
    let total = store.count_concept_maps().await?;

    let params: SearchParams = query.into();
    let results = store.search_concept_maps(&params).await?;

    let bundle = serde_json::json!({
        "resourceType": "Bundle",
        "type": "searchset",
        "total": total,
        "entry": results.iter().map(|cm| {
            serde_json::json!({
                "resource": cm.content.0,
                "search": {
                    "mode": "match"
                }
            })
        }).collect::<Vec<_>>()
    });

    Ok(Json(bundle))
}
