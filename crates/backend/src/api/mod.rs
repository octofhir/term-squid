mod operations;
mod parameters;
mod resources;

use crate::store::TerminologyStore;
use axum::{extract::State, routing::get, Json, Router};
use operations::*;
use resources::*;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn create_router(store: Arc<dyn TerminologyStore>) -> Router {
    // Create version-specific routers
    let version_router = create_version_router();

    Router::new()
        // System endpoints (non-versioned)
        .route("/health", get(health_check))
        .route("/stats", get(get_stats))
        // R4 versioned endpoints
        .nest("/r4", version_router.clone())
        // R5 versioned endpoints
        .nest("/r5", version_router.clone())
        // R6 versioned endpoints
        .nest("/r6", version_router)
        .with_state(store)
}

fn create_version_router() -> Router<Arc<dyn TerminologyStore>> {
    Router::new()
        // Capability endpoints
        .route("/metadata", get(capability_statement))
        .route("/TerminologyCapabilities", get(terminology_capabilities))
        // Resource endpoints
        .merge(codesystem_routes())
        .merge(valueset_routes())
        .merge(conceptmap_routes())
        // Operation endpoints
        .merge(operation_routes())
}

async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "service": "term-squid"
    }))
}

async fn get_stats(State(store): State<Arc<dyn TerminologyStore>>) -> Json<Value> {
    let code_systems_count = store.count_code_systems().await.unwrap_or(0);
    let value_sets_count = store.count_value_sets().await.unwrap_or(0);
    let concept_maps_count = store.count_concept_maps().await.unwrap_or(0);

    Json(json!({
        "code_systems": code_systems_count,
        "value_sets": value_sets_count,
        "concept_maps": concept_maps_count
    }))
}
