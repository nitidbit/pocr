//! Requires a disposable SQL Server with `tests/dataset_fixture.sql` installed.

use std::collections::BTreeMap;
use std::time::Duration;

use infra_caboodle::{CaboodleClient, CaboodleConfig, execute_dataset};
use serde_json::json;

use pocr_core::dataset::{ContextRecord, DatasetDefinition, FieldName};

const TIMEOUT: Duration = Duration::from_mins(5);

fn config() -> CaboodleConfig {
    let required = |key: &str| {
        std::env::var(format!("DATASET_TEST_SQLSERVER_{key}"))
            .expect("explicit SQL Server test configuration required")
    };
    CaboodleConfig {
        host: required("HOST"),
        port: required("PORT").parse().expect("numeric port"),
        database: required("DATABASE"),
        username: required("USERNAME"),
        password: secrecy::SecretString::from(required("PASSWORD")),
    }
}

fn definition() -> DatasetDefinition {
    serde_json::from_value(json!({
        "version": 1, "name": "contract-test", "title": "Contract test", "source": "caboodle",
        "stored_procedure": {"schema": "dbo", "name": "PocrDatasetContractTest"},
        "fields": [
            {"name": "note", "type": "string", "column": "note"},
            {"name": "amount", "type": "number", "column": "amount"},
            {"name": "flag", "type": "boolean", "column": "flag"}
        ]
    }))
    .expect("valid dataset definition")
}

fn record() -> ContextRecord {
    ContextRecord {
        randomization_id: "00000000-0000-0000-0000-000000000001"
            .parse()
            .expect("valid UUID"),
        randomization_time: chrono::Utc::now(),
        context: BTreeMap::from([("note".into(), json!("患者'\"; SELECT 1; --".repeat(1000)))]),
    }
}

async fn client() -> CaboodleClient {
    CaboodleClient::connect(&config())
        .await
        .expect("SQL Server connection")
}

#[tokio::test]
#[ignore = "requires disposable SQL Server and explicitly supplied DATASET_TEST_SQLSERVER_* settings"]
async fn unicode_context_round_trips_as_bound_parameter() {
    let input = record();

    let rows = execute_dataset(
        &mut client().await,
        &definition(),
        std::slice::from_ref(&input),
        TIMEOUT,
    )
    .await
    .expect("dataset execution");

    let note = FieldName::parse("note").expect("valid field name");
    assert_eq!(rows[0].values()[&note], input.context["note"]);
}

#[tokio::test]
#[ignore = "requires disposable SQL Server and explicitly supplied DATASET_TEST_SQLSERVER_* settings"]
async fn typed_columns_convert_to_json_values() {
    let rows = execute_dataset(&mut client().await, &definition(), &[record()], TIMEOUT)
        .await
        .expect("dataset execution");

    let amount = FieldName::parse("amount").expect("valid field name");
    let flag = FieldName::parse("flag").expect("valid field name");
    assert_eq!(rows[0].values()[&amount], json!(1.25));
    assert_eq!(rows[0].values()[&flag], json!(true));
}

#[tokio::test]
#[ignore = "requires disposable SQL Server and explicitly supplied DATASET_TEST_SQLSERVER_* settings"]
async fn second_result_set_is_rejected() {
    let mut input = record();
    input.context.insert("extra_result".into(), json!(true));

    execute_dataset(&mut client().await, &definition(), &[input], TIMEOUT)
        .await
        .unwrap_err();
}

#[tokio::test]
#[ignore = "requires disposable SQL Server and explicitly supplied DATASET_TEST_SQLSERVER_* settings"]
async fn missing_result_row_is_rejected() {
    let mut input = record();
    input.context.insert("omit".into(), json!(true));

    execute_dataset(&mut client().await, &definition(), &[input], TIMEOUT)
        .await
        .unwrap_err();
}

#[tokio::test]
#[ignore = "requires disposable SQL Server and explicitly supplied DATASET_TEST_SQLSERVER_* settings"]
async fn empty_input_yields_no_rows() {
    let rows = execute_dataset(&mut client().await, &definition(), &[], TIMEOUT)
        .await
        .expect("dataset execution");

    assert_eq!(rows, []);
}
