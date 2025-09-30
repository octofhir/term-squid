mod expand;
mod lookup;
mod subsumes;
mod translate;
mod validate;

pub use expand::*;
pub use lookup::*;
pub use subsumes::*;
pub use translate::*;
pub use validate::*;

use crate::store::TerminologyStore;
use axum::{routing::get, Router};
use std::sync::Arc;

pub fn operation_routes() -> Router<Arc<dyn TerminologyStore>> {
    Router::new()
        // CodeSystem operations
        .route("/CodeSystem/$lookup", get(lookup_get).post(lookup_post))
        .route(
            "/CodeSystem/{id}/$lookup",
            get(lookup_instance_get).post(lookup_instance_post),
        )
        .route(
            "/CodeSystem/$validate-code",
            get(validate_code_cs_get).post(validate_code_cs_post),
        )
        .route(
            "/CodeSystem/{id}/$validate-code",
            get(validate_code_cs_instance_get).post(validate_code_cs_instance_post),
        )
        .route(
            "/CodeSystem/$subsumes",
            get(subsumes_get).post(subsumes_post),
        )
        .route(
            "/CodeSystem/{id}/$subsumes",
            get(subsumes_instance_get).post(subsumes_instance_post),
        )
        // ValueSet operations
        .route("/ValueSet/$expand", get(expand_get).post(expand_post))
        .route(
            "/ValueSet/{id}/$expand",
            get(expand_instance_get).post(expand_instance_post),
        )
        .route(
            "/ValueSet/$validate-code",
            get(validate_code_vs_get).post(validate_code_vs_post),
        )
        .route(
            "/ValueSet/{id}/$validate-code",
            get(validate_code_vs_instance_get).post(validate_code_vs_instance_post),
        )
        // ConceptMap operations
        .route(
            "/ConceptMap/$translate",
            get(translate_get).post(translate_post),
        )
        .route(
            "/ConceptMap/{id}/$translate",
            get(translate_instance_get).post(translate_instance_post),
        )
}
