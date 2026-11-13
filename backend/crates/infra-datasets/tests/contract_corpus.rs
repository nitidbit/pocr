//! The shared contract corpus agrees with both runtime validation and the JSON schemas.

use pocr_core::dataset::{Catalog, DatasetDefinition, DatasetRegistry, ReportDefinition};

/// Mirrors loading: parse both definitions, then add the report to a
/// catalog holding the dataset.
fn accepted_by_runtime(dataset: serde_json::Value, report: serde_json::Value) -> bool {
    let Ok(dataset) = serde_json::from_value::<DatasetDefinition>(dataset) else {
        return false;
    };
    let Ok(report) = serde_json::from_value::<ReportDefinition>(report) else {
        return false;
    };
    let mut datasets = DatasetRegistry::default();
    datasets.insert(dataset).expect("single dataset");
    Catalog::new(datasets).insert_report(report).is_ok()
}

/// Clones the corpus base and replaces the value at the case's pointer.
/// An empty pointer replaces the whole target document.
fn apply_case(corpus: &serde_json::Value, case: &serde_json::Value) -> serde_json::Value {
    let mut base = corpus["base"].clone();
    let target = case["target"].as_str().expect("target");
    let pointer = case["pointer"].as_str().expect("pointer");
    *base[target].pointer_mut(pointer).expect("pointer resolves") = case["value"].clone();
    base
}

fn load_corpus() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../datasets/tests/contracts.json"))
        .expect("corpus parses")
}

#[test]
fn contract_corpus_matches_runtime_validation() {
    let corpus = load_corpus();
    for case in corpus["cases"].as_array().expect("cases") {
        let base = apply_case(&corpus, case);

        let is_valid = accepted_by_runtime(base["dataset"].clone(), base["report"].clone());

        assert_eq!(
            is_valid,
            case["valid"].as_bool().expect("valid"),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn contract_corpus_matches_json_schemas() {
    let compile = |schema: &str| {
        let schema = serde_json::from_str(schema).expect("schema parses");
        jsonschema::validator_for(&schema).expect("schema compiles")
    };
    let dataset = compile(include_str!("../../../datasets/dataset.schema.json"));
    let report = compile(include_str!("../../../datasets/report.schema.json"));
    let corpus = load_corpus();
    for case in corpus["cases"].as_array().expect("cases") {
        let target = case["target"].as_str().expect("target");
        let validator = match target {
            "dataset" => &dataset,
            "report" => &report,
            other => panic!("unknown target {other}"),
        };
        let base = apply_case(&corpus, case);

        let errors: Vec<String> = validator
            .iter_errors(&base[target])
            .map(|error| error.to_string())
            .collect();

        assert_eq!(
            errors.is_empty(),
            case["schema_valid"].as_bool().expect("schema_valid"),
            "{}: {errors:?}",
            case["name"]
        );
    }
}
