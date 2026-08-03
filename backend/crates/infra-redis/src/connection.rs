use redis::Client;
use redis::aio::MultiplexedConnection;
use tokio::sync::OnceCell;

/// A thin wrapper around a Redis client providing a shared async connection.
pub struct RedisPool {
    client: Client,
    connection: OnceCell<MultiplexedConnection>,
}

impl RedisPool {
    /// Create a new pool from a Redis URL (e.g., `redis://default:pass@host:6379`).
    pub fn new(url: &str) -> Result<Self, redis::RedisError> {
        let client = Client::open(url)?;
        Ok(Self {
            client,
            connection: OnceCell::new(),
        })
    }

    /// Get or lazily initialize the multiplexed connection.
    pub async fn connection(&self) -> Result<MultiplexedConnection, redis::RedisError> {
        self.connection
            .get_or_try_init(|| async { self.client.get_multiplexed_async_connection().await })
            .await
            .cloned()
    }
}
