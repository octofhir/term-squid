//! Common R4 JSON negotiation. No session, database or terminology work is performed.
use crate::error::AppError;
use axum::{
    extract::Request,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
const OFFERS: [&str; 4] = [
    "application/fhir+json",
    "application/json",
    "text/json",
    "application/json+fhir",
];
fn error(status: StatusCode, message: &str) -> AppError {
    AppError::InvalidInput(status, message.into())
}
fn compatible(value: &mime::Mime) -> bool {
    value
        .get_param("fhirVersion")
        .is_none_or(|v| matches!(v.as_str(), "4.0" | "4.0.1"))
        && value
            .get_param("charset")
            .is_none_or(|v| v.as_str().eq_ignore_ascii_case("utf-8"))
}
pub(super) fn format(value: &str) -> Result<String, AppError> {
    let value = if value == "json" {
        "application/fhir+json"
    } else {
        value
    };
    let parsed: mime::Mime = value.parse().map_err(|_| {
        error(
            StatusCode::NOT_ACCEPTABLE,
            "Unsupported FHIR representation",
        )
    })?;
    if !OFFERS.contains(&parsed.essence_str()) || !compatible(&parsed) {
        return Err(error(
            StatusCode::NOT_ACCEPTABLE,
            "Only R4 UTF-8 JSON representations are supported",
        ));
    }
    Ok(response_type(
        parsed.essence_str(),
        parsed.get_param("fhirVersion").is_some(),
    ))
}
fn response_type(essence: &str, version: bool) -> String {
    if version {
        format!("{essence}; fhirVersion=4.0")
    } else {
        essence.to_owned()
    }
}
fn split_ranges(value: &str) -> Result<Vec<&str>, AppError> {
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    let mut ranges = Vec::new();
    for (index, ch) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
        } else if ch == ',' && !quoted {
            ranges.push(value[start..index].trim());
            start = index + 1;
        }
    }
    if quoted || escaped {
        return Err(error(StatusCode::BAD_REQUEST, "Malformed Accept header"));
    }
    ranges.push(value[start..].trim());
    Ok(ranges)
}
fn valid_quality(value: &str) -> bool {
    if matches!(value, "0" | "1") {
        return true;
    }
    let Some((whole, fraction)) = value.split_once('.') else {
        return false;
    };
    fraction.len() <= 3
        && fraction.bytes().all(|b| b.is_ascii_digit())
        && (whole == "0" || whole == "1" && fraction.bytes().all(|b| b == b'0'))
}
fn range_constraints(range: &mime::Mime) -> Option<(usize, bool)> {
    let mut parameters = 0;
    let mut version = false;
    for (name, value) in range.params() {
        if name.as_str() == "q" {
            break;
        }
        parameters += 1;
        match name.as_str() {
            "charset" if value.as_str().eq_ignore_ascii_case("utf-8") => {}
            "fhirversion" | "fhirVersion" if matches!(value.as_str(), "4.0" | "4.0.1") => {
                version = true;
            }
            _ => return None,
        }
    }
    Some((parameters, version))
}
fn accept(headers: &HeaderMap) -> Result<String, AppError> {
    if !headers.contains_key(header::ACCEPT) {
        return Ok(OFFERS[0].into());
    }
    let mut ranges = Vec::new();
    for header in headers.get_all(header::ACCEPT) {
        let header = header
            .to_str()
            .map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid Accept header"))?;
        for range in split_ranges(header)? {
            if ranges.len() >= 64 {
                return Err(error(StatusCode::BAD_REQUEST, "Too many Accept ranges"));
            }
            let parsed: mime::Mime = range
                .parse()
                .map_err(|_| error(StatusCode::BAD_REQUEST, "Malformed Accept range"))?;
            if parsed
                .params()
                .filter(|(name, _)| name.as_str() == "q")
                .count()
                > 1
            {
                return Err(error(StatusCode::BAD_REQUEST, "Duplicate Accept quality"));
            }
            let quality = parsed
                .get_param("q")
                .map(|v| v.as_str().parse::<f64>())
                .transpose()
                .map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid Accept quality"))?
                .unwrap_or(1.0);
            if parsed
                .get_param("q")
                .is_some_and(|v| !valid_quality(v.as_str()))
                || !quality.is_finite()
                || !(0.0..=1.0).contains(&quality)
            {
                return Err(error(StatusCode::BAD_REQUEST, "Invalid Accept quality"));
            }
            ranges.push((parsed, quality));
        }
    }
    let mut selected = None;
    for offer in OFFERS {
        let native: mime::Mime = offer.parse().expect("Static JSON MIME");
        let mut preference = None;
        for (range, quality) in &ranges {
            let Some((parameters, version)) = range_constraints(range) else {
                continue;
            };
            if range.type_() != mime::STAR && range.type_() != native.type_() {
                continue;
            }
            if range.suffix().is_some() && range.suffix() != native.suffix()
                || range.subtype() != mime::STAR
                    && (range.subtype() != native.subtype() || range.suffix() != native.suffix())
            {
                continue;
            }
            let specificity = (
                if range.type_() == mime::STAR {
                    0
                } else if range.subtype() == mime::STAR {
                    1
                } else {
                    2
                },
                parameters,
            );
            if preference
                .as_ref()
                .is_none_or(|(old, _, _)| specificity > *old)
            {
                preference = Some((specificity, *quality, version));
            }
        }
        if let Some((_, quality, version)) = preference {
            if quality > 0.0 && selected.as_ref().is_none_or(|(best, _)| quality > *best) {
                selected = Some((quality, response_type(offer, version)));
            }
        }
    }
    selected.map(|(_, mime)| mime).ok_or_else(|| {
        error(
            StatusCode::NOT_ACCEPTABLE,
            "No acceptable R4 JSON representation",
        )
    })
}
fn posted_type(headers: &HeaderMap, method: &Method) -> Result<(), AppError> {
    if !matches!(*method, Method::POST | Method::PUT | Method::PATCH) {
        return Ok(());
    }
    if let Some(value) = headers.get(header::CONTENT_TYPE) {
        let value = value
            .to_str()
            .map_err(|_| error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Invalid Content-Type"))?;
        let mime: mime::Mime = value
            .parse()
            .map_err(|_| error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "Invalid Content-Type"))?;
        if !OFFERS.contains(&mime.essence_str()) || !compatible(&mime) {
            return Err(error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Expected R4 UTF-8 FHIR JSON",
            ));
        }
    }
    Ok(())
}
pub(super) async fn negotiate(mut request: Request, next: Next) -> Response {
    let selected = (|| {
        posted_type(request.headers(), request.method())?;
        let query = request.uri().query().unwrap_or("");
        if query.len() > 65536 {
            return Err(error(StatusCode::URI_TOO_LONG, "FHIR query exceeds 64 KiB"));
        }
        let mut override_format = None;
        let mut count = 0;
        for (name, value) in form_urlencoded::parse(query.as_bytes()) {
            count += 1;
            if count > 64 {
                return Err(error(
                    StatusCode::BAD_REQUEST,
                    "FHIR query exceeds 64 parameters",
                ));
            }
            if name == "_format" {
                if override_format.is_some() {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "Duplicate _format parameter",
                    ));
                }
                override_format = Some(format(&value)?);
            }
        }
        override_format
            .map(Ok)
            .unwrap_or_else(|| accept(request.headers()))
    })();
    let selected = match selected {
        Ok(value) => value,
        Err(error) => return error.into_response(),
    };
    // Axum's JSON decoder recognizes formal and generic JSON media types; normalize
    // the supported legacy JSON spellings after validating their original parameters.
    if matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH
    ) && request.headers().contains_key(header::CONTENT_TYPE)
    {
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/fhir+json"),
        );
    }
    let mut response = next.run(request).await;
    if response
        .headers()
        .get(header::CONTENT_TYPE)
        .is_some_and(|v| v.as_bytes().windows(4).any(|w| w == b"json"))
    {
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&selected).expect("Validated JSON MIME"),
        );
    }
    response
        .headers_mut()
        .append(header::VARY, HeaderValue::from_static("Accept"));
    response
}
