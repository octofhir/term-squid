//! Cookie sessions and bounded imports are isolated from the public terminology router.
mod jobs;
#[cfg(test)]
mod tests;
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone)]
pub struct AdminConfig {
    pub username: String,
    pub password_hash: String,
    pub origin: String,
    pub secure: bool,
    pub ttl_seconds: i64,
    pub staging: PathBuf,
}
impl AdminConfig {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Ok(file) = std::env::var("ADMIN_PASSWORD_HASH_FILE") else {
            return Ok(None);
        };
        let password_hash = std::fs::read_to_string(file)?.trim().to_string();
        let parsed = PasswordHash::new(&password_hash)
            .map_err(|_| anyhow::anyhow!("Invalid administrator password hash"))?;
        anyhow::ensure!(
            parsed.algorithm.as_str() == "argon2id",
            "Administrator hash must use Argon2id"
        );
        let origin = std::env::var("ADMIN_ORIGIN")?;
        let uri: axum::http::Uri = origin.parse()?;
        anyhow::ensure!(
            uri.authority().is_some()
                && uri.path() == "/"
                && uri.query().is_none()
                && matches!(uri.scheme_str(), Some("http" | "https")),
            "ADMIN_ORIGIN must be an exact HTTP(S) origin without path"
        );
        let secure = uri.scheme_str() == Some("https");
        anyhow::ensure!(
            secure || matches!(uri.host(), Some("localhost" | "127.0.0.1" | "[::1]")),
            "Non-loopback administration requires HTTPS"
        );
        let ttl_seconds = std::env::var("ADMIN_SESSION_SECONDS")
            .unwrap_or_else(|_| "28800".into())
            .parse()?;
        anyhow::ensure!(
            (30..=86400).contains(&ttl_seconds),
            "ADMIN_SESSION_SECONDS must be 30..86400"
        );
        Ok(Some(Self {
            username: std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into()),
            password_hash,
            origin,
            secure,
            ttl_seconds,
            staging: std::env::var_os("ADMIN_STAGING_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(".term-squid-staging")),
        }))
    }
}

pub struct AdminState {
    pub pool: PgPool,
    pub config: Option<AdminConfig>,
    pub uploads: Semaphore,
    pub password_checks: Semaphore,
    pub available: AtomicBool,
}
pub type Shared = Arc<AdminState>;

#[derive(Debug)]
pub struct ApiError(pub StatusCode, pub &'static str);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":{"message":self.1}}))).into_response()
    }
}
impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "Administration database operation failed");
        Self(
            StatusCode::SERVICE_UNAVAILABLE,
            "Administration is temporarily unavailable",
        )
    }
}
type Result<T> = std::result::Result<T, ApiError>;
fn unavailable() -> ApiError {
    ApiError(
        StatusCode::SERVICE_UNAVAILABLE,
        "Administration is not configured",
    )
}
fn unauthorized() -> ApiError {
    ApiError(StatusCode::UNAUTHORIZED, "Administrator session required")
}
pub fn fingerprint(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
fn random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn cookie_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .map(str::trim)
        .find_map(|s| s.strip_prefix("term_squid_session="))
}
fn check_origin(config: &AdminConfig, headers: &HeaderMap) -> Result<()> {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some(config.origin.as_str()) {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "Same-origin administration required",
        ));
    }
    Ok(())
}
#[derive(sqlx::FromRow)]
struct Session {
    username: String,
    csrf_token: String,
    expires_at: chrono::DateTime<chrono::Utc>,
}
async fn session(state: &AdminState, headers: &HeaderMap) -> Result<Session> {
    if !state.available.load(Ordering::SeqCst) {
        return Err(unavailable());
    }
    let config = state.config.as_ref().ok_or_else(unavailable)?;
    let token = cookie_token(headers)
        .filter(|t| t.len() == 64)
        .ok_or_else(unauthorized)?;
    sqlx::query_as("SELECT username,csrf_token,expires_at FROM admin_sessions WHERE token_hash=$1 AND expires_at>NOW() AND credential_fingerprint=$2 AND username=$3")
        .bind(fingerprint(token)).bind(fingerprint(&config.password_hash)).bind(&config.username)
        .fetch_optional(&state.pool).await?.ok_or_else(unauthorized)
}
async fn protect(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let (parts, body) = request.into_parts();
    let result = async {
        let identity = session(&state, &parts.headers).await?;
        if !matches!(
            parts.method,
            axum::http::Method::GET | axum::http::Method::HEAD
        ) {
            check_origin(
                state.config.as_ref().ok_or_else(unavailable)?,
                &parts.headers,
            )?;
            if parts
                .headers
                .get("x-csrf-token")
                .and_then(|v| v.to_str().ok())
                != Some(identity.csrf_token.as_str())
            {
                return Err(ApiError(StatusCode::FORBIDDEN, "Invalid CSRF token"));
            }
        }
        Ok::<_, ApiError>(())
    }
    .await;
    match result {
        Ok(()) => next.run(Request::from_parts(parts, body)).await,
        Err(e) => e.into_response(),
    }
}
async fn no_cache(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    if response.status().is_client_error()
        && !response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes().starts_with(b"application/json"))
    {
        response = ApiError(response.status(), "Invalid administration request").into_response();
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}

pub async fn router(pool: PgPool, config: Option<AdminConfig>) -> anyhow::Result<Router> {
    let state = Arc::new(AdminState {
        pool,
        config,
        uploads: Semaphore::new(2),
        password_checks: Semaphore::new(1),
        available: AtomicBool::new(true),
    });
    jobs::initialize(state.clone()).await?;
    Ok(routes(state))
}

fn routes(state: Shared) -> Router {
    let private = Router::new()
        .route("/logout", post(logout))
        .route("/jobs", get(jobs::list).post(jobs::upload))
        .route("/jobs/{id}", get(jobs::detail))
        .route("/jobs/{id}/start", post(jobs::start))
        .route("/jobs/{id}/cancel", post(jobs::cancel))
        .route_layer(middleware::from_fn_with_state(state.clone(), protect));
    Router::new()
        .merge(private)
        .route("/login", post(login))
        .route("/session", get(current))
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "Unknown administration endpoint") })
        .layer(DefaultBodyLimit::disable())
        .layer(middleware::from_fn(no_cache))
        .with_state(state)
}
async fn current(State(state): State<Shared>, headers: HeaderMap) -> Result<Json<Value>> {
    if !state.available.load(Ordering::SeqCst) {
        return Err(unavailable());
    }
    if state.config.is_none() {
        return Ok(Json(json!({"configured":false,"administrator":null})));
    }
    match session(&state, &headers).await {
        Ok(s) => Ok(Json(
            json!({"configured":true,"administrator":{"username":s.username},"csrfToken":s.csrf_token,"expiresAt":s.expires_at,"capabilities":["import"]}),
        )),
        Err(ApiError(StatusCode::UNAUTHORIZED, _)) => {
            Ok(Json(json!({"configured":true,"administrator":null})))
        }
        Err(e) => Err(e),
    }
}
#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}
async fn login(
    State(state): State<Shared>,
    headers: HeaderMap,
    request: Request,
) -> Result<Response> {
    if !state.available.load(Ordering::SeqCst) {
        return Err(unavailable());
    }
    let config = state.config.as_ref().ok_or_else(unavailable)?;
    check_origin(config, &headers)?;
    let _permit = state
        .password_checks
        .try_acquire()
        .map_err(|_| ApiError(StatusCode::TOO_MANY_REQUESTS, "Login busy; retry later"))?;
    let bytes = axum::body::to_bytes(request.into_body(), 4096)
        .await
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Invalid login body"))?;
    let input: Credentials = serde_json::from_slice(&bytes)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "Invalid login JSON"))?;
    // Global persistent budget intentionally ignores untrusted proxy/client IP headers.
    let attempts: i32 = sqlx::query_scalar("UPDATE admin_login_limit SET attempts=CASE WHEN window_start < NOW()-INTERVAL '1 minute' THEN 1 ELSE LEAST(attempts+1,6) END, window_start=CASE WHEN window_start < NOW()-INTERVAL '1 minute' THEN NOW() ELSE window_start END RETURNING attempts")
        .fetch_one(&state.pool).await?;
    if attempts > 5 {
        return Err(ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many login attempts; retry in one minute",
        ));
    }
    let hash = config.password_hash.clone();
    let password = input.password;
    let valid = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .map_err(|_| unavailable())?;
    if !valid || input.username != config.username {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }
    let token = random_token();
    let csrf = random_token();
    let expires = chrono::Utc::now() + chrono::Duration::seconds(config.ttl_seconds);
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM admin_sessions WHERE expires_at<=NOW() OR credential_fingerprint<>$1")
        .bind(fingerprint(&config.password_hash))
        .execute(&mut *tx)
        .await?;
    // Bound session history and revoke the oldest sessions on a new sign-in.
    sqlx::query("DELETE FROM admin_sessions WHERE token_hash IN (SELECT token_hash FROM admin_sessions ORDER BY expires_at DESC OFFSET 19)").execute(&mut *tx).await?;
    sqlx::query("INSERT INTO admin_sessions(token_hash,username,credential_fingerprint,csrf_token,expires_at) VALUES($1,$2,$3,$4,$5)")
        .bind(fingerprint(&token)).bind(&config.username).bind(fingerprint(&config.password_hash)).bind(csrf).bind(expires).execute(&mut *tx).await?;
    tx.commit().await?;
    let cookie = format!(
        "term_squid_session={token}; HttpOnly; SameSite=Strict; Path=/api/admin; Max-Age={}{}",
        config.ttl_seconds,
        if config.secure { "; Secure" } else { "" }
    );
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({"ok":true}))).into_response())
}
async fn logout(State(state): State<Shared>, headers: HeaderMap) -> Result<Response> {
    if let Some(token) = cookie_token(&headers) {
        sqlx::query("DELETE FROM admin_sessions WHERE token_hash=$1")
            .bind(fingerprint(token))
            .execute(&state.pool)
            .await?;
    }
    Ok((
        [(
            header::SET_COOKIE,
            "term_squid_session=; HttpOnly; SameSite=Strict; Path=/api/admin; Max-Age=0",
        )],
        Json(json!({"ok":true})),
    )
        .into_response())
}
