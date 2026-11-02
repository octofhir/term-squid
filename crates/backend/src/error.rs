use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Unsupported terminology expression: {0}")]
    Terminology(#[from] term_squid_core::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("Not found: {0}")]
    NotFound(String),

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
            AppError::Internal(ref e) => {
                tracing::error!("Internal error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            }
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "Resource not found"),
            AppError::Terminology(_) => (StatusCode::UNPROCESSABLE_ENTITY, "not-supported"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "Bad request"),
        };

        let body = Json(json!({
            "resourceType": "OperationOutcome",
            "issue": [{"severity": "error", "code": match status {
                StatusCode::NOT_FOUND => "not-found", StatusCode::BAD_REQUEST => "invalid",
                StatusCode::UNPROCESSABLE_ENTITY => "not-supported", _ => "exception"
            }, "diagnostics": if status.is_server_error() { error_message.to_string() } else { self.to_string() }}],
        }));

        (status, body).into_response()
    }
}
