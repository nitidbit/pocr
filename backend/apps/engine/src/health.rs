//! Health check adapter — implements HealthCheckable for Redis.

use std::sync::Arc;

use infra_redis::RedisPool;

/// Adapter to check Redis connectivity via the `HealthCheckable` trait.
pub struct RedisHealthCheck {
    pool: Arc<RedisPool>,
}

impl RedisHealthCheck {
    pub fn new(pool: Arc<RedisPool>) -> Self {
        Self { pool }
    }
}

impl api_http::HealthCheckable for RedisHealthCheck {
    fn ping(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let mut conn = self.pool.connection().await.map_err(|e| e.to_string())?;
            redis::cmd("PING")
                .query_async::<String>(&mut conn)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        })
    }
}
