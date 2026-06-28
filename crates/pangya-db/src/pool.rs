//! Connection-pool setup.

use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;
use thiserror::Error;

/// A shared, cloned-cheaply MySQL connection pool.
pub type DbPool = MySqlPool;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("failed to connect to database: {0}")]
    Connect(#[from] sqlx::Error),
}

/// Create a pool from a `mysql://user:pass@ip:port/db` connect URL.
///
/// Mirrors the C++ `NormalManagerDB` thread pool (26 workers), but as async
/// connections managed by sqlx. `max_connections` defaults to that count.
pub async fn connect(url: &str) -> Result<DbPool, DbError> {
    connect_with(url, 26).await
}

/// Create a pool with an explicit connection cap (useful in tests).
pub async fn connect_with(url: &str, max_connections: u32) -> Result<DbPool, DbError> {
    let pool = MySqlPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await?;
    Ok(pool)
}
