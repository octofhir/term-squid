//! Avoid gzip state for small known bodies without violating content negotiation.
use axum::{
    body::{Body, HttpBody},
    extract::Request,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use futures_util::{future::Either, future::Map, FutureExt};
use std::{
    convert::Infallible,
    sync::Arc,
    task::{Context, Poll},
};
use tower::{util::BoxCloneSyncService, util::Oneshot, Layer, Service, ServiceExt};
use tower_http::compression::{
    predicate::{NotForContentType, Predicate, SizeAbove},
    CompressionLayer,
};

const MIN_BYTES: u64 = 1024;

/// Keep ordinary identity requests on a typed path without negotiation wrappers.
#[derive(Clone, Copy)]
pub(crate) struct TransportLayer;

#[derive(Clone)]
pub(crate) struct Transport<S> {
    inner: S,
    negotiated: Arc<BoxCloneSyncService<Request, Response, Infallible>>,
}

impl<S> Layer<S> for TransportLayer
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + Sync + 'static,
    S::Future: Send + 'static,
{
    type Service = Transport<S>;

    fn layer(&self, inner: S) -> Self::Service {
        let negotiated = axum::middleware::from_fn(compression).layer(inner.clone());
        Transport {
            inner,
            negotiated: Arc::new(BoxCloneSyncService::new(negotiated)),
        }
    }
}

type ResponseResult = Result<Response, Infallible>;
type IdentityFuture<F> = Map<F, fn(ResponseResult) -> ResponseResult>;

fn identity_response(response: ResponseResult) -> ResponseResult {
    response.map(|mut response| {
        vary_encoding(response.headers_mut());
        response
    })
}

impl<S> Service<Request> for Transport<S>
where
    S: Service<Request, Response = Response, Error = Infallible> + Clone + Send + Sync + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = Either<
        IdentityFuture<S::Future>,
        Oneshot<BoxCloneSyncService<Request, Response, Infallible>, Request>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        if !request.headers().contains_key(header::ACCEPT_ENCODING) {
            return Either::Left(
                self.inner
                    .call(request)
                    .map(identity_response as fn(_) -> _),
            );
        }
        // Release any readiness reservation before the independently readied
        // negotiated clone runs (important for shared concurrency limits).
        let replacement = self.inner.clone();
        drop(std::mem::replace(&mut self.inner, replacement));
        Either::Right(self.negotiated.as_ref().clone().oneshot(request))
    }
}

#[derive(Clone)]
struct Policy {
    identity_forbidden: bool,
    head: bool,
}
impl Predicate for Policy {
    fn should_compress<B: HttpBody>(&self, response: &axum::http::Response<B>) -> bool {
        if response.status().is_informational()
            || matches!(
                response.status(),
                StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED
            )
        {
            return false;
        }
        // Retain the locked tower-http default MIME exclusions. Unknown-size
        // streaming bodies remain eligible; no response is buffered for sizing.
        NotForContentType::GRPC
            .and(NotForContentType::IMAGES)
            .and(NotForContentType::SSE)
            .should_compress(response)
            && !response.headers().contains_key(header::CONTENT_RANGE)
            && (self.identity_forbidden
                || if self.head {
                    // Endpoint middleware runs before axum's top-level HEAD
                    // stripping and automatic Content-Length insertion. An
                    // explicit HEAD handler may instead return an empty body
                    // with the selected representation's length in its header.
                    response
                        .headers()
                        .get(header::CONTENT_LENGTH)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .or_else(|| response.body().size_hint().exact())
                        .is_none_or(|size| size >= MIN_BYTES)
                } else {
                    SizeAbove::new(MIN_BYTES).should_compress(response)
                })
    }
}

fn encoding_quality(headers: &HeaderMap, wanted: &str) -> u16 {
    let wanted = if wanted.eq_ignore_ascii_case("x-gzip") {
        "gzip"
    } else {
        wanted
    };
    if !headers.contains_key(header::ACCEPT_ENCODING) {
        return 1000;
    }
    let mut identity = None;
    let mut wildcard = None;
    for header in headers.get_all(header::ACCEPT_ENCODING) {
        let Ok(value) = header.to_str() else { continue };
        for item in value.split(',') {
            let mut fields = item.split(';');
            let coding = fields.next().unwrap_or_default().trim();
            let matches = coding.eq_ignore_ascii_case(wanted)
                || wanted.eq_ignore_ascii_case("gzip") && coding.eq_ignore_ascii_case("x-gzip");
            if !matches && coding != "*" {
                continue;
            }
            let mut quality = 1000;
            for field in fields {
                let Some((key, value)) = field.trim().split_once('=') else {
                    continue;
                };
                if !key.trim().eq_ignore_ascii_case("q") {
                    continue;
                }
                let value = value.trim();
                let parsed = match value.split_once('.') {
                    None if value == "0" => Some(0),
                    None if value == "1" => Some(1000),
                    Some((whole, fraction))
                        if fraction.len() <= 3 && fraction.bytes().all(|b| b.is_ascii_digit()) =>
                    {
                        match whole {
                            "0" => Some(
                                fraction.parse::<u16>().unwrap_or(0)
                                    * 10u16.pow((3 - fraction.len()) as u32),
                            ),
                            "1" if fraction.bytes().all(|b| b == b'0') => Some(1000),
                            _ => None,
                        }
                    }
                    _ => None,
                };
                if let Some(parsed) = parsed {
                    quality = parsed;
                }
            }
            let slot = if matches {
                &mut identity
            } else {
                &mut wildcard
            };
            *slot = Some(slot.unwrap_or(0).max(quality));
        }
    }
    identity
        .or(wildcard)
        .unwrap_or(if wanted.eq_ignore_ascii_case("identity") {
            1000
        } else {
            0
        })
}

fn vary_encoding(headers: &mut HeaderMap) {
    if !headers
        .get_all(header::VARY)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .any(|v| v.trim() == "*" || v.trim().eq_ignore_ascii_case("accept-encoding"))
    {
        headers.append(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
}

fn not_acceptable() -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NOT_ACCEPTABLE;
    vary_encoding(response.headers_mut());
    response
}

fn strip_head_body(response: &mut Response, preserve_length: bool) {
    if preserve_length && !response.headers().contains_key(header::CONTENT_LENGTH) {
        if let Some(size) = response.body().size_hint().exact() {
            response.headers_mut().insert(
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&size.to_string()).expect("A body length is a valid header"),
            );
        }
    }
    // A known-empty body would make axum invent Content-Length: 0 before its
    // final HEAD stripping, even when the selected encoded length is unknown.
    *response.body_mut() = Body::from_stream(futures_util::stream::empty::<
        Result<axum::body::Bytes, Infallible>,
    >());
}

pub(crate) async fn compression(request: Request, next: Next) -> Response {
    // Keep only negotiation metadata across handler execution. The response body
    // stays an unpolled stream; bodyless/preencoded responses bypass the encoder.
    let forbidden = encoding_quality(request.headers(), "identity") == 0;
    let head = request.method() == Method::HEAD;
    let mut negotiation = Request::new(Body::empty());
    *negotiation.method_mut() = request.method().clone();
    for value in request.headers().get_all(header::ACCEPT_ENCODING) {
        negotiation
            .headers_mut()
            .append(header::ACCEPT_ENCODING, value.clone());
    }
    let mut response = next.run(request).await;
    if response.status().is_informational()
        || matches!(
            response.status(),
            StatusCode::NO_CONTENT | StatusCode::NOT_MODIFIED
        )
    {
        *response.body_mut() = Body::empty();
        vary_encoding(response.headers_mut());
        return response;
    }
    if response.headers().contains_key(header::CONTENT_ENCODING) {
        let acceptable = response
            .headers()
            .get_all(header::CONTENT_ENCODING)
            .iter()
            .all(|value| {
                value.to_str().ok().is_some_and(|value| {
                    value.split(',').all(|coding| {
                        !coding.trim().is_empty()
                            && encoding_quality(negotiation.headers(), coding.trim()) > 0
                    })
                })
            });
        if !acceptable {
            return not_acceptable();
        }
        if head {
            strip_head_body(&mut response, true);
        }
        vary_encoding(response.headers_mut());
        return response;
    }
    let policy = Policy {
        identity_forbidden: forbidden,
        head,
    };
    if head {
        // HEAD describes the selected GET representation but must not allocate a
        // compressor or consume a body. An explicit identity preference wins.
        let gzip = encoding_quality(negotiation.headers(), "gzip");
        let identity = encoding_quality(negotiation.headers(), "identity");
        let weighted_identity = negotiation
            .headers()
            .get_all(header::ACCEPT_ENCODING)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .any(|v| {
                let coding = v.split(';').next().unwrap_or_default().trim();
                coding.eq_ignore_ascii_case("identity") || coding == "*"
            });
        let compress = negotiation.headers().contains_key(header::ACCEPT_ENCODING)
            && gzip > 0
            && (!weighted_identity || gzip >= identity)
            && policy.should_compress(&response);
        if compress {
            response
                .headers_mut()
                .insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
            response.headers_mut().remove(header::CONTENT_LENGTH);
        } else if forbidden {
            return not_acceptable();
        }
        strip_head_body(&mut response, !compress);
        vary_encoding(response.headers_mut());
        return response;
    }
    let original_status = response.status();
    let mut response = Some(response);
    let service = tower::service_fn(move |_: Request| {
        std::future::ready(Ok::<_, Infallible>(
            response.take().expect("Single compression call"),
        ))
    });
    let response = CompressionLayer::new()
        .compress_when(policy)
        .layer(service)
        .oneshot(negotiation)
        .await;
    let mut response = match response {
        Ok(response) => response.map(Body::new),
        Err(error) => match error {},
    };
    // tower-http can set 406 before consulting a predicate. Never forward its
    // original, now unacceptable representation under the negotiation error.
    if (response.status() == StatusCode::NOT_ACCEPTABLE
        && original_status != StatusCode::NOT_ACCEPTABLE)
        || forbidden && !response.headers().contains_key(header::CONTENT_ENCODING)
    {
        return not_acceptable();
    }
    vary_encoding(response.headers_mut());
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use http_body_util::BodyExt;
    use std::io::Read;

    fn app(payload: Vec<u8>, mime: &'static str) -> Router {
        Router::new()
            .route(
                "/resource",
                get(move || {
                    let payload = payload.clone();
                    async move { ([(header::CONTENT_TYPE, mime)], payload) }
                }),
            )
            .layer(TransportLayer)
    }
    async fn request(app: Router, encoding: Option<&str>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut request = Request::builder().uri("/resource");
        if let Some(value) = encoding {
            request = request.header(header::ACCEPT_ENCODING, value);
        }
        let response = app
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        (status, headers, bytes)
    }
    fn decoded(headers: &HeaderMap, body: &[u8]) -> Vec<u8> {
        if headers
            .get(header::CONTENT_ENCODING)
            .is_some_and(|v| v == "gzip")
        {
            let mut value = Vec::new();
            flate2::read::GzDecoder::new(body)
                .read_to_end(&mut value)
                .unwrap();
            value
        } else {
            body.to_vec()
        }
    }
    #[tokio::test]
    async fn missing_accept_encoding_uses_the_typed_future_and_shares_slow_service() {
        let inner = tower::service_fn(|_: Request| async {
            Ok::<_, Infallible>(Response::new(Body::from("typed identity body")))
        });
        let mut service = TransportLayer.layer(inner);
        let cloned = service.clone();
        assert!(Arc::ptr_eq(&service.negotiated, &cloned.negotiated));
        let future = service
            .ready()
            .await
            .unwrap()
            .call(Request::new(Body::empty()));
        assert!(matches!(&future, Either::Left(_)));
        let response = future.await.unwrap();
        assert_eq!(
            response.headers().get(header::VARY).unwrap(),
            "accept-encoding"
        );
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            "typed identity body"
        );
        let request = Request::builder()
            .header(header::ACCEPT_ENCODING, "")
            .body(Body::empty())
            .unwrap();
        let future = service.ready().await.unwrap().call(request);
        assert!(matches!(&future, Either::Right(_)));
        assert_eq!(future.await.unwrap().status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn default_identity_preserves_get_head_vary_preencoded_and_bodyless_responses() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, b"preencoded representation").unwrap();
        let encoded = encoder.finish().unwrap();
        for status in [
            StatusCode::OK,
            StatusCode::NO_CONTENT,
            StatusCode::NOT_MODIFIED,
        ] {
            for preencoded in [false, true] {
                for method in [Method::GET, Method::HEAD] {
                    let payload = if status != StatusCode::OK {
                        Vec::new()
                    } else if preencoded {
                        encoded.clone()
                    } else {
                        vec![b'x'; 4096]
                    };
                    let expected = payload.clone();
                    let router = Router::new()
                        .route(
                            "/resource",
                            get(move || {
                                let payload = payload.clone();
                                async move {
                                    let mut response = Response::new(Body::from(payload));
                                    *response.status_mut() = status;
                                    response
                                        .headers_mut()
                                        .insert(header::VARY, HeaderValue::from_static("Origin"));
                                    if preencoded {
                                        response.headers_mut().insert(
                                            header::CONTENT_ENCODING,
                                            HeaderValue::from_static("gzip"),
                                        );
                                    }
                                    response
                                }
                            }),
                        )
                        .layer(TransportLayer);
                    let response = router
                        .oneshot(
                            Request::builder()
                                .method(method.clone())
                                .uri("/resource")
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(response.status(), status);
                    assert_eq!(
                        response.headers().contains_key(header::CONTENT_ENCODING),
                        preencoded
                    );
                    let vary: Vec<_> = response
                        .headers()
                        .get_all(header::VARY)
                        .iter()
                        .map(|value| value.to_str().unwrap())
                        .collect();
                    assert_eq!(vary, ["Origin", "accept-encoding"]);
                    if status == StatusCode::OK {
                        assert_eq!(
                            response
                                .headers()
                                .get(header::CONTENT_LENGTH)
                                .unwrap()
                                .to_str()
                                .unwrap(),
                            expected.len().to_string()
                        );
                    }
                    let actual = response.into_body().collect().await.unwrap().to_bytes();
                    if method == Method::HEAD {
                        assert!(actual.is_empty());
                    } else {
                        assert_eq!(actual.as_ref(), expected);
                    }
                }
            }
        }
        let router = Router::new()
            .route(
                "/resource",
                get(|| async { StatusCode::OK }).head(|| async { StatusCode::ACCEPTED }),
            )
            .layer(TransportLayer);
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/resource")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        assert!(response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty());
    }
    #[tokio::test]
    async fn known_small_bodies_skip_gzip_and_large_bodies_preserve_decoded_bytes() {
        for size in [0, 31, 32, 288, 1023, 1024, 4096] {
            let payload = vec![b'x'; size];
            let (status, headers, body) =
                request(app(payload.clone(), "application/fhir+json"), Some("gzip")).await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                headers.contains_key(header::CONTENT_ENCODING),
                size >= 1024,
                "{size}"
            );
            assert_eq!(decoded(&headers, &body), payload);
            assert!(headers
                .get_all(header::VARY)
                .iter()
                .any(|v| v.to_str().unwrap().contains("accept-encoding")));
            if size >= 1024 {
                assert!(!headers.contains_key(header::CONTENT_LENGTH));
            }
        }
    }
    #[tokio::test]
    async fn explicit_identity_refusal_forces_eligible_small_gzip_without_losing_payload() {
        for value in [
            "gzip, identity;q=0",
            "gzip, identity;q=0.000",
            "gzip, *;q=0",
            "GZip, IDENTITY;Q=0",
        ] {
            for size in [0, 20, 288] {
                let payload = vec![b'x'; size];
                let (status, headers, body) =
                    request(app(payload.clone(), "application/fhir+json"), Some(value)).await;
                assert_eq!(status, StatusCode::OK, "{value}");
                assert_eq!(headers.get(header::CONTENT_ENCODING).unwrap(), "gzip");
                assert_eq!(decoded(&headers, &body), payload);
            }
        }
    }
    #[tokio::test]
    async fn unsupported_or_forbidden_codings_do_not_silently_send_identity() {
        for value in ["gzip;q=0, identity;q=0", "br, identity;q=0", "*;q=0"] {
            let (status, headers, body) =
                request(app(vec![b'x'; 288], "application/fhir+json"), Some(value)).await;
            assert_eq!(status, StatusCode::NOT_ACCEPTABLE, "{value}");
            assert!(body.is_empty());
            assert!(!headers.contains_key(header::CONTENT_ENCODING));
        }
    }
    #[tokio::test]
    async fn identity_defaults_and_specific_overrides_remain_acceptable() {
        for value in [
            None,
            Some(""),
            Some("identity"),
            Some("gzip;q=0"),
            Some("gzip, *;q=0, identity;q=1"),
            Some("gzip, identity;q=0.5, *;q=0"),
        ] {
            let payload = vec![b'x'; 288];
            let (status, headers, body) =
                request(app(payload.clone(), "application/fhir+json"), value).await;
            assert_eq!(status, StatusCode::OK);
            assert!(!headers.contains_key(header::CONTENT_ENCODING));
            assert_eq!(body, payload);
        }
    }
    #[tokio::test]
    async fn default_mime_exclusions_remain_uncompressed_or_explicitly_unacceptable() {
        for mime in ["image/png", "text/event-stream", "application/grpc"] {
            let payload = vec![b'x'; 4096];
            let (status, headers, body) = request(app(payload.clone(), mime), Some("gzip")).await;
            assert_eq!(status, StatusCode::OK);
            assert!(!headers.contains_key(header::CONTENT_ENCODING));
            assert_eq!(body, payload);
            let (status, _, body) = request(app(payload, mime), Some("gzip,identity;q=0")).await;
            assert_eq!(status, StatusCode::NOT_ACCEPTABLE);
            assert!(body.is_empty());
        }
    }
    #[tokio::test]
    async fn unknown_size_streams_stay_gzip_eligible_and_repeated_header_fields_are_respected() {
        let payload = b"streamed FHIR-shaped content".to_vec();
        let expected = payload.clone();
        let router = Router::new()
            .route(
                "/resource",
                get(move || {
                    let data = payload.clone();
                    async move {
                        (
                            [(header::CONTENT_TYPE, "application/fhir+json")],
                            Body::from_stream(futures_util::stream::iter([Ok::<_, Infallible>(
                                data,
                            )])),
                        )
                    }
                }),
            )
            .layer(TransportLayer);
        let (status, headers, body) = request(router, Some("gzip")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers.get(header::CONTENT_ENCODING).unwrap(), "gzip");
        assert_eq!(decoded(&headers, &body), expected);
        let router = app(vec![b'x'; 20], "application/fhir+json");
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/resource")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .header(header::ACCEPT_ENCODING, "identity;q=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING).unwrap(),
            "gzip"
        );
    }
    #[tokio::test]
    async fn bodyless_status_and_head_preserve_http_semantics_when_identity_is_refused() {
        for status in [StatusCode::NO_CONTENT, StatusCode::NOT_MODIFIED] {
            for encoding in ["gzip,identity;q=0", "br,identity;q=0", "*;q=0"] {
                let router = Router::new()
                    .route("/resource", get(move || async move { status }))
                    .layer(TransportLayer);
                let (actual, headers, body) = request(router, Some(encoding)).await;
                assert_eq!(actual, status);
                assert!(body.is_empty());
                assert!(!headers.contains_key(header::CONTENT_ENCODING));
            }
        }
        let response = app(vec![b'x'; 288], "application/fhir+json")
            .oneshot(
                Request::builder()
                    .method(Method::HEAD)
                    .uri("/resource")
                    .header(header::ACCEPT_ENCODING, "gzip,identity;q=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .is_empty());
    }
    #[tokio::test]
    async fn existing_vary_tokens_and_preencoded_responses_are_not_overwritten() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut encoder, b"already encoded source").unwrap();
        let body = encoder.finish().unwrap();
        let original = body.clone();
        let router = Router::new()
            .route(
                "/resource",
                get(move || {
                    let body = body.clone();
                    async move {
                        (
                            [
                                (header::CONTENT_TYPE, "application/fhir+json"),
                                (header::CONTENT_ENCODING, "gzip"),
                                (header::VARY, "Origin, Accept-Encoding"),
                            ],
                            body,
                        )
                    }
                }),
            )
            .layer(TransportLayer);
        let (status, headers, encoded) = request(router, Some("gzip")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(encoded, original);
        assert_eq!(decoded(&headers, &encoded), b"already encoded source");
        assert_eq!(headers.get_all(header::VARY).iter().count(), 1);
        assert_eq!(
            headers.get(header::VARY).unwrap(),
            "Origin, Accept-Encoding"
        );
    }
    #[tokio::test]
    async fn preencoded_codings_are_checked_against_actual_request_preferences() {
        for (coding, accept, status) in [
            ("gzip", "br,identity;q=0", StatusCode::NOT_ACCEPTABLE),
            ("br", "gzip,identity;q=0", StatusCode::NOT_ACCEPTABLE),
            ("br", "br,identity;q=0", StatusCode::OK),
            ("identity", "gzip,identity;q=0", StatusCode::NOT_ACCEPTABLE),
        ] {
            let router =
                Router::new()
                    .route(
                        "/resource",
                        get(move || async move {
                            ([(header::CONTENT_ENCODING, coding)], "encoded source")
                        }),
                    )
                    .layer(TransportLayer);
            let (actual, _, body) = request(router, Some(accept)).await;
            assert_eq!(actual, status, "{coding} {accept}");
            if status == StatusCode::NOT_ACCEPTABLE {
                assert!(body.is_empty());
            } else {
                assert_eq!(body, b"encoded source");
            }
        }
    }
    #[tokio::test]
    async fn head_agrees_with_get_coding_for_eligible_and_excluded_representations() {
        for mime in ["application/fhir+json", "image/png", "text/event-stream"] {
            for size in [0, 288, 1023, 1024, 4096] {
                for accept in [
                    "gzip",
                    "x-gzip",
                    "X-GZip,identity;q=0",
                    "gzip,identity;q=0",
                    "br,identity;q=0",
                    "gzip;q=0.1,identity;q=0.8",
                    "gzip;q=0.1,*;q=0.8",
                    "gzip,*;q=0",
                ] {
                    let router = app(vec![b'x'; size], mime);
                    let (status, headers, _) = request(router.clone(), Some(accept)).await;
                    let response = router
                        .oneshot(
                            Request::builder()
                                .method(Method::HEAD)
                                .uri("/resource")
                                .header(header::ACCEPT_ENCODING, accept)
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(response.status(), status, "{mime} {size} {accept}");
                    assert_eq!(
                        response.headers().get(header::CONTENT_ENCODING),
                        headers.get(header::CONTENT_ENCODING),
                        "{mime} {size} {accept}"
                    );
                    if status != StatusCode::NOT_ACCEPTABLE {
                        if headers.contains_key(header::CONTENT_ENCODING) {
                            assert!(
                                !response.headers().contains_key(header::CONTENT_LENGTH),
                                "{mime} {size} {accept}"
                            );
                        } else {
                            assert_eq!(
                                response.headers().get(header::CONTENT_LENGTH),
                                headers.get(header::CONTENT_LENGTH),
                                "{mime} {size} {accept}"
                            );
                        }
                    }
                    assert!(response
                        .into_body()
                        .collect()
                        .await
                        .unwrap()
                        .to_bytes()
                        .is_empty());
                }
            }
        }
    }
    #[tokio::test]
    async fn explicit_head_handlers_are_preserved_and_never_poll_the_body() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        for length in [Some(288), Some(4096), None] {
            let get_calls = Arc::new(AtomicUsize::new(0));
            let head_calls = Arc::new(AtomicUsize::new(0));
            let body_polls = Arc::new(AtomicUsize::new(0));
            let get_count = get_calls.clone();
            let head_count = head_calls.clone();
            let poll_count = body_polls.clone();
            let router = Router::new()
                .route(
                    "/resource",
                    get(move || {
                        get_count.fetch_add(1, Ordering::SeqCst);
                        async { "GET must not replace an explicit HEAD handler" }
                    })
                    .head(move || {
                        head_count.fetch_add(1, Ordering::SeqCst);
                        let poll_count = poll_count.clone();
                        async move {
                            let stream = futures_util::stream::poll_fn(move |_| {
                                poll_count.fetch_add(1, Ordering::SeqCst);
                                std::task::Poll::Ready(
                                    None::<Result<axum::body::Bytes, Infallible>>,
                                )
                            });
                            let mut response = Response::new(Body::from_stream(stream));
                            *response.status_mut() = StatusCode::ACCEPTED;
                            response.headers_mut().insert(
                                header::CONTENT_TYPE,
                                HeaderValue::from_static("application/fhir+json"),
                            );
                            response
                                .headers_mut()
                                .insert("x-handler", HeaderValue::from_static("explicit-head"));
                            if let Some(length) = length {
                                response.headers_mut().insert(
                                    header::CONTENT_LENGTH,
                                    HeaderValue::from_str(&length.to_string()).unwrap(),
                                );
                            }
                            response
                        }
                    }),
                )
                .layer(TransportLayer);
            let response = router
                .oneshot(
                    Request::builder()
                        .method(Method::HEAD)
                        .uri("/resource")
                        .header(header::ACCEPT_ENCODING, "gzip")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            assert_eq!(
                response.headers().get("x-handler").unwrap(),
                "explicit-head"
            );
            let gzip = length.is_none_or(|length| length >= MIN_BYTES);
            assert_eq!(
                response.headers().contains_key(header::CONTENT_ENCODING),
                gzip
            );
            if gzip {
                assert!(!response.headers().contains_key(header::CONTENT_LENGTH));
            } else {
                assert_eq!(
                    response.headers().get(header::CONTENT_LENGTH).unwrap(),
                    "288"
                );
            }
            assert!(response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty());
            assert_eq!(get_calls.load(Ordering::SeqCst), 0);
            assert_eq!(head_calls.load(Ordering::SeqCst), 1);
            assert_eq!(body_polls.load(Ordering::SeqCst), 0);
        }
    }
}
