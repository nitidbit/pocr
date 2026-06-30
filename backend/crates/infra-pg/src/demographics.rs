//! Patient demographics persistence for the Caboodle sync pipeline.
//!
//! Provides bulk upsert and query operations against the
//! `patient_demographics` table. The balance endpoint uses
//! [`PgDemographicsRepository::find_by_study`] to retrieve demographics
//! joined with randomization arm assignments.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::instrument;
use uuid::Uuid;

use crate::error::PgError;

/// Patient demographic record synced from Caboodle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientDemographics {
    /// Epic patient identifier (MRN or internal ID).
    pub patient_id: String,
    pub birth_date: Option<NaiveDate>,
    pub sex: Option<String>,
    pub races: Option<Vec<String>>,
    pub ethnicity: Option<String>,
    pub primary_language: Option<String>,
    pub smoking_status: Option<String>,
    pub insurance_type: Option<String>,
    pub bmi: Option<f64>,
    pub synced_at: DateTime<Utc>,
}

/// Demographics joined with randomization arm assignment for a study.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatientDemographicsWithArm {
    pub patient_id: String,
    pub birth_date: Option<NaiveDate>,
    pub sex: Option<String>,
    pub races: Option<Vec<String>>,
    pub ethnicity: Option<String>,
    pub primary_language: Option<String>,
    pub smoking_status: Option<String>,
    pub insurance_type: Option<String>,
    pub bmi: Option<f64>,
    pub synced_at: DateTime<Utc>,
    /// Arm UUID from the randomization record.
    pub arm_id: Uuid,
}

/// PostgreSQL-backed demographics repository.
pub struct PgDemographicsRepository {
    pool: PgPool,
}

impl PgDemographicsRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Bulk upsert patient demographics from a Caboodle sync batch.
    ///
    /// Use `ON CONFLICT DO UPDATE` to refresh all fields and bump
    /// `synced_at` on each upsert.
    ///
    /// # Errors
    ///
    /// Return [`PgError::Database`] on connection or query failure.
    #[instrument(name = "demographics.upsert_batch", skip(self, batch), fields(batch_size = batch.len()))]
    pub async fn upsert_batch(&self, batch: &[PatientDemographics]) -> Result<(), PgError> {
        if batch.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for record in batch {
            let races: Option<&[String]> = record.races.as_deref();

            sqlx::query(
                r#"INSERT INTO patient_demographics
                       (patient_id, birth_date, sex, races, ethnicity,
                        primary_language, smoking_status, insurance_type, bmi, synced_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                   ON CONFLICT (patient_id) DO UPDATE SET
                       birth_date = EXCLUDED.birth_date,
                       sex = EXCLUDED.sex,
                       races = EXCLUDED.races,
                       ethnicity = EXCLUDED.ethnicity,
                       primary_language = EXCLUDED.primary_language,
                       smoking_status = EXCLUDED.smoking_status,
                       insurance_type = EXCLUDED.insurance_type,
                       bmi = EXCLUDED.bmi,
                       synced_at = EXCLUDED.synced_at"#,
            )
            .bind(&record.patient_id)
            .bind(record.birth_date)
            .bind(&record.sex)
            .bind(races)
            .bind(&record.ethnicity)
            .bind(&record.primary_language)
            .bind(&record.smoking_status)
            .bind(&record.insurance_type)
            .bind(record.bmi)
            .bind(record.synced_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        tracing::info!(count = batch.len(), "demographics batch upserted");
        Ok(())
    }

    /// Fetch demographics for a set of patient IDs.
    ///
    /// # Errors
    ///
    /// Return [`PgError::Database`] on connection or query failure.
    #[instrument(name = "demographics.find_by_patient_ids", skip(self, patient_ids), fields(count = patient_ids.len()))]
    pub async fn find_by_patient_ids(
        &self,
        patient_ids: &[String],
    ) -> Result<Vec<PatientDemographics>, PgError> {
        if patient_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query_as::<_, DemographicsRow>(
            "SELECT * FROM patient_demographics WHERE patient_id = ANY($1)",
        )
        .bind(patient_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Fetch demographics for all randomized patients in a study, joined
    /// with their arm assignment.
    ///
    /// Join `patient_demographics.patient_id = randomizations.epic_patient_id`
    /// where `randomizations.study_id = study_id`. Extract the arm UUID
    /// from the JSONB `arm` column's `id` field.
    ///
    /// # Errors
    ///
    /// Return [`PgError::Database`] on connection or query failure.
    #[instrument(name = "demographics.find_by_study", skip(self))]
    pub async fn find_by_study(
        &self,
        study_id: &Uuid,
    ) -> Result<Vec<PatientDemographicsWithArm>, PgError> {
        let rows = sqlx::query_as::<_, DemographicsWithArmRow>(
            r#"SELECT
                   pd.patient_id,
                   pd.birth_date,
                   pd.sex,
                   pd.races,
                   pd.ethnicity,
                   pd.primary_language,
                   pd.smoking_status,
                   pd.insurance_type,
                   pd.bmi,
                   pd.synced_at,
                   (r.arm->>'id')::UUID AS arm_id
               FROM patient_demographics pd
               JOIN randomizations r ON r.epic_patient_id = pd.patient_id
               WHERE r.study_id = $1"#,
        )
        .bind(study_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

// -- Row types for sqlx mapping --

#[derive(sqlx::FromRow)]
struct DemographicsRow {
    patient_id: String,
    birth_date: Option<NaiveDate>,
    sex: Option<String>,
    races: Option<Vec<String>>,
    ethnicity: Option<String>,
    primary_language: Option<String>,
    smoking_status: Option<String>,
    insurance_type: Option<String>,
    bmi: Option<f64>,
    synced_at: DateTime<Utc>,
}

impl From<DemographicsRow> for PatientDemographics {
    fn from(row: DemographicsRow) -> Self {
        Self {
            patient_id: row.patient_id,
            birth_date: row.birth_date,
            sex: row.sex,
            races: row.races,
            ethnicity: row.ethnicity,
            primary_language: row.primary_language,
            smoking_status: row.smoking_status,
            insurance_type: row.insurance_type,
            bmi: row.bmi,
            synced_at: row.synced_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct DemographicsWithArmRow {
    patient_id: String,
    birth_date: Option<NaiveDate>,
    sex: Option<String>,
    races: Option<Vec<String>>,
    ethnicity: Option<String>,
    primary_language: Option<String>,
    smoking_status: Option<String>,
    insurance_type: Option<String>,
    bmi: Option<f64>,
    synced_at: DateTime<Utc>,
    arm_id: Uuid,
}

impl From<DemographicsWithArmRow> for PatientDemographicsWithArm {
    fn from(row: DemographicsWithArmRow) -> Self {
        Self {
            patient_id: row.patient_id,
            birth_date: row.birth_date,
            sex: row.sex,
            races: row.races,
            ethnicity: row.ethnicity,
            primary_language: row.primary_language,
            smoking_status: row.smoking_status,
            insurance_type: row.insurance_type,
            bmi: row.bmi,
            synced_at: row.synced_at,
            arm_id: row.arm_id,
        }
    }
}
