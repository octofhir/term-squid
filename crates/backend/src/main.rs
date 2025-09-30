mod api;
mod config;
mod error;
mod models;
mod store;

use anyhow::Result;
use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use config::Config;
use rust_embed::Embed;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use store::{PostgresStore, TerminologyStore};
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "term_squid=debug,tower_http=debug,axum=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("Configuration loaded");
    tracing::info!("Server will bind to: {}", config.bind_address());

    // Create database connection pool
    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    tracing::info!("Database connection pool created");

    // Test database connection
    sqlx::query("SELECT 1").execute(&pool).await?;
    tracing::info!("Database connection verified");

    // Create store
    let store: Arc<dyn TerminologyStore> = Arc::new(PostgresStore::new(pool));
    tracing::info!("PostgreSQL store initialized");

    // Build application router with embedded static files
    let app = api::create_router(store).fallback(static_handler).layer(
        tower::ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(CompressionLayer::new())
            .layer(CorsLayer::permissive()),
    );

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.bind_address()).await?;
    tracing::info!("Server listening on {}", config.bind_address());

    axum::serve(listener, app).await?;

    Ok(())
}

// Handler for serving embedded static files
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // If the path is empty or is a known SPA route, serve index.html
    if path.is_empty() || !path.contains('.') {
        return serve_index_html();
    }

    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_str(mime.as_ref()).unwrap(),
                )
                .body(Body::from(content.data))
                .unwrap()
        }
        None => serve_index_html(),
    }
}

fn serve_index_html() -> Response {
    match StaticAssets::get("index.html") {
        Some(content) => Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/html"),
            )
            .body(Body::from(content.data))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("404 Not Found"))
            .unwrap(),
    }
}
