//! Pool policy shared by server startup and database integration checks.
use sqlx::{postgres::PgPoolOptions, Connection};
use std::time::Duration;

const IDLE_HEALTH_CHECK_AFTER: Duration = Duration::from_secs(1);

pub fn pool_options(max_connections: u32) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(max_connections)
        // Like OctoFHIR, avoid a protocol round trip on every hot checkout.
        .test_before_acquire(false)
        .before_acquire(|connection, metadata| {
            Box::pin(async move {
                // Revalidate idle sockets; errors discard the socket before the request uses it.
                if metadata.idle_for >= IDLE_HEALTH_CHECK_AFTER {
                    connection.ping().await?;
                }
                Ok(true)
            })
        })
}
