//! Requires a disposable SQL Server with tests/dataset_fixture.sql installed.

use std::time::Duration;

use infra_caboodle::{CaboodleClient, CaboodleConfig, datasets::execute_dataset};
use serde_json::json;

use pocr_core::dataset::{ContextRecord, DatasetDefinition, FieldName};

const TIMEOUT: Duration = Duration::from_mins(5);

#[tokio::test]
#[ignore = "requires disposable SQL Server and explicitly supplied DATASET_TEST_SQLSERVER_* settings"]
async fn bound_unicode_json_and_strict_result_contract() {
    let required = |key: &str| {
        std::env::var(format!("DATASET_TEST_SQLSERVER_{key}"))
            .expect("explicit SQL Server test configuration required")
    };
    let config = CaboodleConfig {
        host: required("HOST"),
        port: required("PORT").parse().unwrap(),
        database: required("DATABASE"),
        username: required("USERNAME"),
        password: secrecy::SecretString::from(required("PASSWORD")),
    };
    let definition: DatasetDefinition = serde_json::from_value(json!({
        "version":1,"name":"contract-test","title":"Contract test","source":"caboodle",
        "stored_procedure":{"schema":"dbo","name":"PocrDatasetContractTest"},
        "fields": [
            {"name": "note", "type": "string", "column": "note"},
            {"name": "amount", "type": "number", "column": "amount"},
            {"name": "flag", "type": "boolean", "column": "flag"}
        ]
    }))
    .unwrap();
    let mut input = ContextRecord {
        randomization_id: "00000000-0000-0000-0000-000000000001".parse().unwrap(),
        randomization_time: chrono::Utc::now(),
        context: std::collections::BTreeMap::from([(
            "note".into(),
            json!("患者'\"; SELECT 1; --".repeat(1000)),
        )]),
    };
    let rows = execute_dataset(
        &mut CaboodleClient::connect(&config).await.unwrap(),
        &definition,
        &[input.clone()],
        TIMEOUT,
    )
    .await
    .unwrap();
    assert_eq!(
        rows[0].values()[&FieldName::parse("note").unwrap()],
        input.context["note"]
    );
    assert_eq!(
        rows[0].values()[&FieldName::parse("amount").unwrap()],
        json!(1.25)
    );
    assert_eq!(
        rows[0].values()[&FieldName::parse("flag").unwrap()],
        json!(true)
    );
    input.context.insert("extra_result".into(), json!(true));
    execute_dataset(
        &mut CaboodleClient::connect(&config).await.unwrap(),
        &definition,
        &[input.clone()],
        TIMEOUT,
    )
    .await
    .unwrap_err();
    input.context.remove("extra_result");
    input.context.insert("omit".into(), json!(true));
    execute_dataset(
        &mut CaboodleClient::connect(&config).await.unwrap(),
        &definition,
        &[input],
        TIMEOUT,
    )
    .await
    .unwrap_err();
    assert!(
        execute_dataset(
            &mut CaboodleClient::connect(&config).await.unwrap(),
            &definition,
            &[],
            TIMEOUT,
        )
        .await
        .unwrap()
        .is_empty()
    );
}
