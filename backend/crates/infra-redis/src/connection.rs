use redis::Client;
use redis::aio::MultiplexedConnection;
use tokio::sync::OnceCell;

/// Thin wrapper around a Redis client that provides a shared async connection.
pub struct RedisPool {
    client: Client,
    connection: OnceCell<MultiplexedConnection>,
}

impl RedisPool {
    /// Creates a pool from a Redis URL (e.g., `redis://default:pass@host:6379`).
    ///
    /// No connection is opened until [`RedisPool::connection`] is first called.
    ///
    /// # Errors
    ///
    /// Returns an error if `url` is not a valid Redis connection URL.
    pub fn new(url: &str) -> Result<Self, redis::RedisError> {
        let client = Client::open(url)?;
        Ok(Self {
            client,
            connection: OnceCell::new(),
        })
    }

    /// Returns the shared multiplexed connection, opening it on first use.
    ///
    /// # Errors
    ///
    /// Returns an error if the initial connection to Redis cannot be
    /// established.
    pub async fn connection(&self) -> Result<MultiplexedConnection, redis::RedisError> {
        self.connection
            .get_or_try_init(|| async { self.client.get_multiplexed_async_connection().await })
            .await
            .cloned()
    }
}
