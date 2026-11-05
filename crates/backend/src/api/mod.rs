mod coded_input;
mod extractors;
mod operations;
mod parameters;
mod protocol;
mod resources;
mod transient;

use crate::store::TerminologyStore;
use axum::{extract::State, routing::get, Json, Router};
use operations::*;
use resources::*;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn create_router(store: Arc<dyn TerminologyStore>) -> Router {
    // The verified adapter is R4; legacy URL aliases expose the same R4 contract.
    let version_router = create_version_router();

    Router::new()
        // System endpoints (non-versioned)
        .route("/health", get(health_check))
        .route("/stats", get(get_stats))
        // R4 versioned endpoints
        .nest("/r4", version_router.clone())
        // Legacy aliases do not establish R5/R6 adapter support.
        .nest("/r5", version_router.clone())
        .nest("/r6", version_router)
        .with_state(store)
}

fn create_version_router() -> Router<Arc<dyn TerminologyStore>> {
    Router::new()
        // Capability endpoints
        .route("/metadata", get(capability_statement))
        .route("/TerminologyCapabilities", get(terminology_capabilities))
        .route("/OperationDefinition/{id}", get(operation_definition))
        // Resource endpoints
        .merge(codesystem_routes())
        .merge(valueset_routes())
        .merge(conceptmap_routes())
        // Operation endpoints
        .merge(operation_routes())
        .fallback(|| async { crate::error::AppError::NotFound("Unknown FHIR endpoint".into()) })
        .method_not_allowed_fallback(|| async {
            crate::error::AppError::InvalidInput(
                axum::http::StatusCode::METHOD_NOT_ALLOWED,
                "FHIR method not supported for this endpoint".into(),
            )
        })
        .layer(axum::middleware::from_fn(protocol::negotiate))
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "service": "term-squid"
    }))
}

async fn get_stats(
    State(store): State<Arc<dyn TerminologyStore>>,
) -> Result<Json<Value>, crate::error::AppError> {
    let code_systems_count = store.count_code_systems().await?;
    let value_sets_count = store.count_value_sets().await?;
    let concept_maps_count = store.count_concept_maps().await?;

    Ok(Json(json!({
        "code_systems": code_systems_count,
        "value_sets": value_sets_count,
        "concept_maps": concept_maps_count
    })))
}
