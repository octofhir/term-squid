//! FHIR operation extractors: protocol failures remain OperationOutcome responses.
use crate::api::parameters::Parameters;
use crate::error::AppError;
use axum::{
    extract::{FromRequest, FromRequestParts, Path, Request},
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

/// The official Java client appends a transport nonce, including to /metadata.
/// It never changes terminology selection or bypasses the mandatory freshness boundary.
fn transport_query(query: &str) -> Result<std::borrow::Cow<'_, str>, AppError> {
    let mut nonce = false;
    for (name, value) in form_urlencoded::parse(query.as_bytes()) {
        if value.is_empty() {
            return Err(AppError::BadRequest("Empty FHIR query value".into()));
        }
        if name == "nocache" {
            if nonce || value.len() > 20 || value.parse::<u64>().is_err() {
                return Err(AppError::BadRequest(
                    "nocache must be one unsigned integer transport nonce".into(),
                ));
            }
            nonce = true;
        }
    }
    if !nonce {
        return Ok(std::borrow::Cow::Borrowed(query));
    }
    let mut output = form_urlencoded::Serializer::new(String::new());
    for (name, value) in form_urlencoded::parse(query.as_bytes()) {
        if name != "nocache" {
            output.append_pair(&name, &value);
        }
    }
    Ok(std::borrow::Cow::Owned(output.finish()))
}

pub struct FhirParameters(pub Parameters);
impl<S: Send + Sync> FromRequest<S> for FhirParameters {
    type Rejection = AppError;
    async fn from_request(mut request: Request, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(parameters) = request.extensions_mut().remove::<Parameters>() {
            return Ok(Self(parameters));
        }
        let filtered = transport_query(request.uri().query().unwrap_or(""))?;
        let query: PostQuery = serde_html_form::from_str(&filtered).map_err(|_| {
            AppError::BadRequest(
                "POST operation inputs belong in Parameters; only _format and nocache are accepted in the query"
                    .into(),
            )
        })?;
        validate_json_format(query.format.as_deref())?;
        Json::<Parameters>::from_request(request, state)
            .await
            .map_err(|error| {
                let status = match error.status() {
                    StatusCode::UNPROCESSABLE_ENTITY => StatusCode::BAD_REQUEST,
                    other => other,
                };
                AppError::InvalidInput(status, "Invalid FHIR Parameters JSON or media type".into())
            })
            .and_then(|Json(value)| {
                if value.resource_type != "Parameters" {
                    return Err(AppError::BadRequest(
                        "Expected a Parameters resource".into(),
                    ));
                }
                Ok(Self(value))
            })
    }
}

pub struct FhirQuery<T>(pub T);
impl<S: Send + Sync, T: DeserializeOwned + Send> FromRequestParts<S> for FhirQuery<T> {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let _ = state;
        let query = transport_query(parts.uri.query().unwrap_or(""))?;
        serde_html_form::from_str(&query)
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

#[cfg(test)]
mod tests {
    use super::transport_query;
    #[test]
    fn transport_nonce_preserves_repeated_clinical_inputs_and_rejects_ambiguous_values() {
        assert_eq!(
            transport_query("designation=ru&nocache=123&designation=en").unwrap(),
            "designation=ru&designation=en"
        );
        assert_eq!(
            transport_query("code=C00001&unknown=value").unwrap(),
            "code=C00001&unknown=value"
        );
        for invalid in [
            "nocache=",
            "nocache=-1",
            "nocache=1&nocache=2",
            "nocache=18446744073709551616",
            "code=",
        ] {
            assert!(transport_query(invalid).is_err(), "{invalid}");
        }
    }
}
