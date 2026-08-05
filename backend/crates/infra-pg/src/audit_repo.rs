//! Audit event persistence.
//!
//! Implements the append-only [`AuditRepository`] port against the
//! `audit_events` table. Reads return a study's history newest-first.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use pocr_core::audit::AuditEvent;
use pocr_core::ports::{AuditRepository, RepositoryError};

use crate::error::PgError;

/// PostgreSQL-backed audit event repository.
pub struct PgAuditRepository {
    pool: PgPool,
}

impl PgAuditRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditRepository for PgAuditRepository {
    /// Append a new audit event.
    ///
    /// # Errors
    ///
    /// Return [`RepositoryError::Internal`] on connection or query failure.
    #[instrument(name = "audit.append", skip(self, event), fields(study_id = %event.study_id, event_type = %event.event_type))]
    async fn append(&self, event: &AuditEvent) -> Result<(), RepositoryError> {
        sqlx::query(
            r#"INSERT INTO audit_events (id, study_id, event_type, actor, detail, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(event.id)
        .bind(event.study_id)
        .bind(&event.event_type)
        .bind(&event.actor)
        .bind(&event.detail)
        .bind(event.created_at)
        .execute(&self.pool)
        .await
        .map_err(PgError::from)?;
        Ok(())
    }

    /// List a study's audit events, newest-first.
    ///
    /// # Errors
    ///
    /// Return [`RepositoryError::Internal`] on connection or query failure.
    #[instrument(name = "audit.list_by_study", skip(self))]
    async fn list_by_study(&self, study_id: &Uuid) -> Result<Vec<AuditEvent>, RepositoryError> {
        let rows = sqlx::query_as::<_, AuditRow>(
            r#"SELECT id, study_id, event_type, actor, detail, created_at
               FROM audit_events
               WHERE study_id = $1
               ORDER BY created_at DESC, id DESC"#,
        )
        .bind(study_id)
        .fetch_all(&self.pool)
        .await
        .map_err(PgError::from)?;
        Ok(rows.into_iter().map(AuditEvent::from).collect())
    }
}

/// Intermediate row type mapping an `audit_events` record.
#[derive(sqlx::FromRow)]
struct AuditRow {
    id: Uuid,
    study_id: Uuid,
    event_type: String,
    actor: String,
    detail: Value,
    created_at: DateTime<Utc>,
}

impl From<AuditRow> for AuditEvent {
    fn from(row: AuditRow) -> Self {
        AuditEvent::from_stored(
            row.id,
            row.study_id,
            row.event_type,
            row.actor,
            row.detail,
            row.created_at,
        )
    }
}
