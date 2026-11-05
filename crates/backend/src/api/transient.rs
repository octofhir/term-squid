//! Only public read-operation POSTs enter a bounded temporary resource scope.
use super::{extractors::FhirParameters, parameters::Parameters};
use crate::{error::AppError, store::transient};
use axum::{
    body::Body,
    extract::{FromRequest, Request},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock,
};

// Dropping a request or its deadline stops preparation at the next resource boundary.
// A running bounded core preparation keeps its permit until it actually returns.
struct Cancellation(Arc<AtomicBool>);
impl Drop for Cancellation {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
static REQUESTS: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(64)));
static WORK: LazyLock<Arc<tokio::sync::Semaphore>> =
    LazyLock::new(|| Arc::new(tokio::sync::Semaphore::new(4)));

pub async fn scope(request: Request, next: Next) -> Response {
    if request.method() != axum::http::Method::POST {
        return next.run(request).await;
    }
    limited(
        request,
        next,
        REQUESTS.clone(),
        std::time::Duration::from_secs(15),
    )
    .await
}
async fn limited(
    request: Request,
    next: Next,
    requests: Arc<tokio::sync::Semaphore>,
    deadline: std::time::Duration,
) -> Response {
    match tokio::time::timeout(deadline, execute(request, next, requests)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => error.into_response(),
        Err(_) => AppError::Overloaded.into_response(),
    }
}
async fn execute(
    request: Request,
    next: Next,
    requests: Arc<tokio::sync::Semaphore>,
) -> Result<Response, AppError> {
    // Admit before reading/allocating JSON, including slow or malformed bodies.
    let _admission = requests
        .try_acquire_owned()
        .map_err(|_| AppError::Overloaded)?;
    let (parts, body) = request.into_parts();
    let mut restored = parts.clone();
    let FhirParameters(mut parameters) =
        FhirParameters::from_request(Request::from_parts(parts, body), &()).await?;
    let fields = parameters.parameter.get_or_insert_default();
    if fields.len() > 96 {
        return Err(AppError::BadRequest(
            "Operation parameter/context count exceeds 96".into(),
        ));
    }
    let mut resources = Vec::new();
    for parameter in fields.iter_mut().filter(|p| p.name == "tx-resource") {
        if parameter.value.is_some()
            || parameter.part.is_some()
            || !parameter.other.is_empty()
            || parameter.resource.is_none()
        {
            return Err(AppError::BadRequest(
                "tx-resource requires exactly one resource, without value[x] or part".into(),
            ));
        }
        resources.push(parameter.resource.take().unwrap());
    }
    fields.retain(|p| p.name != "tx-resource");
    if resources.is_empty() {
        restored.extensions.insert(parameters);
        return Ok(next.run(Request::from_parts(restored, Body::empty())).await);
    }
    let date = parameters.get_date("date").map(str::to_owned);
    let cancellation = Cancellation(Arc::new(AtomicBool::new(false)));
    let cancelled = cancellation.0.clone();
    let work = WORK
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| AppError::Overloaded)?;
    let (context, work) = tokio::task::spawn_blocking(move || {
        (
            transient::Resources::prepare(resources, date.as_deref(), &cancelled),
            work,
        )
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let context = Arc::new(context?);
    restored.extensions.insert::<Parameters>(parameters);
    Ok(transient::scope(context, async move {
        let _work = work;
        next.run(Request::from_parts(restored, Body::empty())).await
    })
    .await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{http::StatusCode, middleware, routing::post, Router};
    use http_body_util::BodyExt;
    use std::{sync::atomic::AtomicBool, task::Poll, time::Duration};
    use tower::ServiceExt;

    fn app(requests: Arc<tokio::sync::Semaphore>) -> Router {
        Router::new()
            .route(
                "/operation",
                post(|FhirParameters(parameters): FhirParameters| async move {
                    super::super::extractors::FhirJson(parameters)
                }),
            )
            .layer(middleware::from_fn(move |request, next| {
                limited(request, next, requests.clone(), Duration::from_millis(20))
            }))
    }
    fn slow_body(polled: Arc<AtomicBool>) -> Body {
        Body::from_stream(futures_util::stream::poll_fn(move |_| {
            polled.store(true, Ordering::Release);
            Poll::Pending::<Option<Result<axum::body::Bytes, std::io::Error>>>
        }))
    }
    fn request(body: Body) -> Request {
        Request::builder()
            .method("POST")
            .uri("/operation")
            .header("content-type", "application/fhir+json")
            .body(body)
            .unwrap()
    }
    async fn assert_ready(app: Router) {
        let response = app
            .oneshot(request(Body::from(
                r#"{"resourceType":"Parameters","parameter":[]}"#,
            )))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    #[tokio::test]
    async fn saturated_operation_admission_rejects_before_polling_the_body() {
        let requests = Arc::new(tokio::sync::Semaphore::new(1));
        let permit = requests.clone().acquire_owned().await.unwrap();
        let app = app(requests.clone());
        let polled = Arc::new(AtomicBool::new(false));
        let response = app
            .clone()
            .oneshot(request(slow_body(polled.clone())))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            !polled.load(Ordering::Acquire),
            "Overload must reject before reading JSON"
        );
        let body: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(body["resourceType"], "OperationOutcome");
        drop(permit);
        assert_ready(app).await;
        assert_eq!(requests.available_permits(), 1);
    }
    #[tokio::test]
    async fn operation_deadline_includes_a_slow_body_and_releases_admission() {
        let requests = Arc::new(tokio::sync::Semaphore::new(1));
        let app = app(requests.clone());
        let polled = Arc::new(AtomicBool::new(false));
        let response = app
            .clone()
            .oneshot(request(slow_body(polled.clone())))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(polled.load(Ordering::Acquire));
        assert_eq!(requests.available_permits(), 1);
        assert_ready(app).await;
    }
}
