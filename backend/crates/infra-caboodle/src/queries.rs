use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use tiberius::Row;
use tracing::{debug, instrument, warn};

use crate::connection::CaboodleClient;
use crate::error::{CaboodleError, CaboodleResult};

/// Demographics data for a single patient from Caboodle.
#[derive(Debug, Clone)]
pub struct CaboodleDemographics {
    pub patient_epic_id: String,
    pub patient_key: i64,
    pub birth_date: Option<NaiveDate>,
    pub sex: Option<String>,
    pub ethnicity: Option<String>,
    pub primary_language: Option<String>,
    pub smoking_status: Option<String>,
}

/// Fetch core demographics from PatientDim for a batch of Epic patient IDs.
///
/// Return demographics keyed by `PatientEpicId` along with the `PatientKey`
/// for subsequent joins. Results are batched in chunks of 1000 to stay within
/// MS-SQL parameter limits.
#[instrument(skip(client, patient_epic_ids), fields(batch_size = patient_epic_ids.len()))]
pub async fn fetch_demographics(
    client: &mut CaboodleClient,
    patient_epic_ids: &[String],
) -> CaboodleResult<Vec<CaboodleDemographics>> {
    let mut all_results = Vec::new();

    for chunk in patient_epic_ids.chunks(1000) {
        let params: String = chunk
            .iter()
            .enumerate()
            .map(|(i, _)| format!("@P{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            r#"SELECT PatientEpicId, PatientKey, BirthDate, Sex, Ethnicity, FirstLanguage, SmokingStatus
               FROM PatientDim
               WHERE PatientEpicId IN ({params})"#
        );

        let mut q = tiberius::Query::new(query);
        for id in chunk {
            q.bind(id.as_str());
        }

        let stream = q
            .query(&mut client.client)
            .await
            .map_err(|e| CaboodleError::Query(format!("fetch_demographics: {e}")))?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .map_err(|e| CaboodleError::Query(format!("fetch_demographics results: {e}")))?;

        for row in &rows {
            let patient_epic_id: &str = row.get::<&str, _>("PatientEpicId").unwrap_or_default();
            let patient_key: i64 = row.get::<i64, _>("PatientKey").unwrap_or_default();
            let birth_date: Option<NaiveDate> = row.get::<NaiveDate, _>("BirthDate");
            let sex: Option<&str> = row.get::<&str, _>("Sex");
            let ethnicity: Option<&str> = row.get::<&str, _>("Ethnicity");
            let primary_language: Option<&str> = row.get::<&str, _>("FirstLanguage");
            let smoking_status: Option<&str> = row.get::<&str, _>("SmokingStatus");

            all_results.push(CaboodleDemographics {
                patient_epic_id: patient_epic_id.to_string(),
                patient_key,
                birth_date,
                sex: sex.map(String::from),
                ethnicity: ethnicity.map(String::from),
                primary_language: primary_language.map(String::from),
                smoking_status: smoking_status.map(String::from),
            });
        }

        debug!(chunk_size = chunk.len(), results = all_results.len(), "fetched demographics chunk");
    }

    Ok(all_results)
}

/// Fetch race information from PatientRaceBridge joined with RaceDim.
///
/// Return a map of `PatientKey` to list of race names.
#[instrument(skip(client, patient_keys), fields(batch_size = patient_keys.len()))]
pub async fn fetch_races(
    client: &mut CaboodleClient,
    patient_keys: &[i64],
) -> CaboodleResult<HashMap<i64, Vec<String>>> {
    let mut result: HashMap<i64, Vec<String>> = HashMap::new();

    for chunk in patient_keys.chunks(1000) {
        let params: String = chunk
            .iter()
            .enumerate()
            .map(|(i, _)| format!("@P{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            r#"SELECT prb.PatientKey, rd.RaceName
               FROM PatientRaceBridge prb
               INNER JOIN RaceDim rd ON prb.RaceKey = rd.RaceKey
               WHERE prb.PatientKey IN ({params})"#
        );

        let mut q = tiberius::Query::new(query);
        for key in chunk {
            q.bind(*key);
        }

        let stream = q
            .query(&mut client.client)
            .await
            .map_err(|e| CaboodleError::Query(format!("fetch_races: {e}")))?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .map_err(|e| CaboodleError::Query(format!("fetch_races results: {e}")))?;

        for row in &rows {
            let patient_key: i64 = row.get::<i64, _>("PatientKey").unwrap_or_default();
            let race_name: &str = row.get::<&str, _>("RaceName").unwrap_or("Unknown");
            result
                .entry(patient_key)
                .or_default()
                .push(race_name.to_string());
        }
    }

    Ok(result)
}

/// Fetch insurance (financial class) from CoverageDim.
///
/// Return the most recent active financial class for each patient key.
#[instrument(skip(client, patient_keys), fields(batch_size = patient_keys.len()))]
pub async fn fetch_insurance(
    client: &mut CaboodleClient,
    patient_keys: &[i64],
) -> CaboodleResult<HashMap<i64, String>> {
    let mut result: HashMap<i64, String> = HashMap::new();

    for chunk in patient_keys.chunks(1000) {
        let params: String = chunk
            .iter()
            .enumerate()
            .map(|(i, _)| format!("@P{}", i + 1))
            .collect::<Vec<_>>()
            .join(", ");

        let query = format!(
            r#"SELECT c.PatientKey, c.FinancialClass
               FROM (
                   SELECT PatientKey, FinancialClass,
                          ROW_NUMBER() OVER (PARTITION BY PatientKey ORDER BY EffectiveDate DESC) as rn
                   FROM CoverageDim
                   WHERE PatientKey IN ({params}) AND CoverageStatus = 'Active'
               ) c
               WHERE c.rn = 1"#
        );

        let mut q = tiberius::Query::new(query);
        for key in chunk {
            q.bind(*key);
        }

        let stream = q
            .query(&mut client.client)
            .await
            .map_err(|e| CaboodleError::Query(format!("fetch_insurance: {e}")))?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .map_err(|e| CaboodleError::Query(format!("fetch_insurance results: {e}")))?;

        for row in &rows {
            let patient_key: i64 = row.get::<i64, _>("PatientKey").unwrap_or_default();
            if let Some(financial_class) = row.get::<&str, _>("FinancialClass") {
                result.insert(patient_key, financial_class.to_string());
            }
        }
    }

    Ok(result)
}

/// Fetch most recent BMI value before each patient's randomization timestamp.
///
/// Accept `(PatientEpicId, randomization_issued_at)` pairs and return
/// `PatientEpicId` to BMI value.
#[instrument(skip(client, patients), fields(batch_size = patients.len()))]
pub async fn fetch_bmi(
    client: &mut CaboodleClient,
    patients: &[(String, DateTime<Utc>)],
) -> CaboodleResult<HashMap<String, f64>> {
    let mut result: HashMap<String, f64> = HashMap::new();

    for (patient_id, randomization_date) in patients {
        let query = r#"
            SELECT TOP 1 fvf.Value
            FROM FlowsheetValueFact fvf
            INNER JOIN FlowsheetRowDim frd ON fvf.FlowsheetRowKey = frd.FlowsheetRowKey
            INNER JOIN PatientDim pd ON fvf.PatientKey = pd.PatientKey
            WHERE pd.PatientEpicId = @P1
              AND frd.FlowsheetRowEpicId = '14'
              AND fvf.InstantDts <= @P2
            ORDER BY fvf.InstantDts DESC
        "#;

        let mut q = tiberius::Query::new(query);
        q.bind(patient_id.as_str());
        q.bind(randomization_date.naive_utc());

        let stream = q
            .query(&mut client.client)
            .await
            .map_err(|e| CaboodleError::Query(format!("fetch_bmi for {patient_id}: {e}")))?;

        let rows: Vec<Row> = stream
            .into_first_result()
            .await
            .map_err(|e| {
                CaboodleError::Query(format!("fetch_bmi results for {patient_id}: {e}"))
            })?;

        if let Some(row) = rows.first()
            && let Some(value_str) = row.get::<&str, _>("Value")
        {
            match value_str.parse::<f64>() {
                Ok(bmi) => {
                    result.insert(patient_id.clone(), bmi);
                }
                Err(_) => {
                    warn!(patient_id = %patient_id, value = %value_str, "non-numeric BMI value");
                }
            }
        }
    }

    Ok(result)
}
