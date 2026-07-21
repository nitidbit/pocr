//! Health check adapters — implement `HealthCheckable` for PostgreSQL and Redis.

use std::sync::Arc;

use async_trait::async_trait;
use infra_redis::RedisPool;
use sqlx::PgPool;

/// Adapter to check PostgreSQL connectivity via the `HealthCheckable` trait.
pub struct PgHealthCheck {
    pool: PgPool,
}

impl PgHealthCheck {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl pocr_core::ports::HealthCheckable for PgHealthCheck {
    async fn ping(&self) -> Result<(), String> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// Adapter to check Redis connectivity via the `HealthCheckable` trait.
pub struct RedisHealthCheck {
    pool: Arc<RedisPool>,
}

impl RedisHealthCheck {
    pub fn new(pool: Arc<RedisPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl pocr_core::ports::HealthCheckable for RedisHealthCheck {
    async fn ping(&self) -> Result<(), String> {
        let mut conn = self.pool.connection().await.map_err(|e| e.to_string())?;
        redis::cmd("PING")
            .query_async::<String>(&mut conn)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
