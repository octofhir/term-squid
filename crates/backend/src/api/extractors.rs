//! FHIR operation extractors: protocol failures remain OperationOutcome responses.
use crate::api::parameters::Parameters;
use crate::error::AppError;
use axum::{
    extract::{FromRequest, FromRequestParts, Path, Query, Request},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// These operations return the supported FHIR JSON representation explicitly.
pub struct FhirJson<T>(pub T);
impl<T: Serialize> IntoResponse for FhirJson<T> {
    fn into_response(self) -> Response {
        (
            [(axum::http::header::CONTENT_TYPE, "application/fhir+json")],
            Json(self.0),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostQuery {
    #[serde(rename = "_format")]
    format: Option<String>,
}
pub fn validate_json_format(format: Option<&str>) -> Result<(), AppError> {
    format.map(super::protocol::format).transpose().map(|_| ())
}

pub struct FhirParameters(pub Parameters);
impl<S: Send + Sync> FromRequest<S> for FhirParameters {
    type Rejection = AppError;
    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Query(query) = Query::<PostQuery>::try_from_uri(request.uri()).map_err(|_| {
            AppError::BadRequest(
                "POST operation inputs belong in Parameters; only _format is accepted in the query"
                    .into(),
            )
        })?;
        validate_json_format(query.format.as_deref())?;
        Json::<Parameters>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|error| {
                let status = match error.status() {
                    StatusCode::UNPROCESSABLE_ENTITY => StatusCode::BAD_REQUEST,
                    other => other,
                };
                AppError::InvalidInput(status, "Invalid FHIR Parameters JSON or media type".into())
            })
    }
}

pub struct FhirQuery<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for FhirQuery<T> {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let _ = state;
        let query = parts.uri.query().unwrap_or("");
        if form_urlencoded::parse(query.as_bytes()).any(|(_, value)| value.is_empty()) {
            return Err(AppError::BadRequest("Empty FHIR query value".into()));
        }
        serde_html_form::from_str(query)
            .map(Self)
            .map_err(|error| AppError::BadRequest(format!("Invalid FHIR query: {error}")))
    }
}

pub struct FhirPath<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for FhirPath<T> {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|_| AppError::BadRequest("Invalid resource instance identifier".into()))
    }
}
