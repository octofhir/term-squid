use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Internal dispatch signal: never convert native content into a finite empty plan.
    #[error("Native ValueSet requires projection evaluation")]
    NativeValueSet,

    #[error("Unsupported terminology expression: {0}")]
    Terminology(#[from] term_squid_core::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Database generation read failed: {0}")]
    Generation(#[source] std::sync::Arc<sqlx::Error>),

    #[error("Terminology operation capacity or time budget exceeded")]
    Overloaded,

    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid operation input: {1}")]
    InvalidInput(StatusCode, String),

    #[error("Bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Database(ref e) => {
                tracing::error!("Database error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            AppError::Generation(ref e) => {
                tracing::error!("Database generation error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            AppError::Overloaded => (StatusCode::SERVICE_UNAVAILABLE, "Service overloaded"),
            AppError::Internal(ref e) => {
                tracing::error!("Internal error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "Resource not found"),
            AppError::Terminology(_) | AppError::NativeValueSet => {
                (StatusCode::UNPROCESSABLE_ENTITY, "not-supported")
            }
            AppError::InvalidInput(status, _) => (status, "Invalid operation input"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        let body = Json(json!({
            "resourceType": "OperationOutcome",
            "issue": [{"severity": "error", "code": match status {
                StatusCode::NOT_FOUND => "not-found", StatusCode::BAD_REQUEST => "invalid",
                StatusCode::UNPROCESSABLE_ENTITY => "not-supported", StatusCode::PAYLOAD_TOO_LARGE => "too-costly", StatusCode::UNSUPPORTED_MEDIA_TYPE | StatusCode::NOT_ACCEPTABLE | StatusCode::METHOD_NOT_ALLOWED => "not-supported", _ => "exception"
            }, "diagnostics": if status.is_server_error() { error_message.to_string() } else { self.to_string() }}],
        }));

        (
            status,
            [(axum::http::header::CONTENT_TYPE, "application/fhir+json")],
            body,
        )
            .into_response()
    }
}
