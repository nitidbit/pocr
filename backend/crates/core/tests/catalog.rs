//! Catalog invariants: unique names and reports bound to deployed datasets.

use pocr_core::dataset::{
    Catalog, CatalogError, DatasetDefinition, DatasetName, DatasetRegistry, IncompatibleReport,
    ReportDefinition,
};
use serde_json::{Value, json};

fn dataset(name: &str) -> DatasetDefinition {
    serde_json::from_value(json!({
        "version": 1,
        "name": name,
        "title": "Outcomes",
        "source": "caboodle",
        "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
        "fields": [{"name": "los", "type": "number", "column": "LOS"}]
    }))
    .expect("valid dataset")
}

fn report_json(name: &str, dataset: &str) -> Value {
    json!({
        "version": 1,
        "name": name,
        "dataset": dataset,
        "title": "Outcomes",
        "description": "By arm",
        "variables": [{"field": "los", "label": "LOS", "analysis": "continuous"}]
    })
}

fn report(name: &str, dataset: &str) -> ReportDefinition {
    serde_json::from_value(report_json(name, dataset)).expect("valid report")
}

fn catalog() -> Catalog {
    let mut datasets = DatasetRegistry::default();
    datasets.insert(dataset("outcomes")).expect("insert");
    datasets.insert(dataset("other")).expect("insert");
    Catalog::new(datasets)
}

#[test]
fn registry_get_finds_inserted_definition_by_name() {
    let mut registry = DatasetRegistry::default();
    registry.insert(dataset("outcomes")).expect("insert");
    let name = DatasetName::parse("outcomes").expect("name");

    assert_eq!(registry.get(&name), Some(&dataset("outcomes")));
}

#[test]
fn registry_is_empty_until_a_definition_is_inserted() {
    let mut registry = DatasetRegistry::default();
    let was_empty = registry.is_empty();

    registry.insert(dataset("outcomes")).expect("insert");

    assert!(was_empty);
    assert!(!registry.is_empty());
}

#[test]
fn duplicate_dataset_is_rejected() {
    let mut registry = DatasetRegistry::default();
    registry.insert(dataset("outcomes")).expect("first insert");

    let error = registry.insert(dataset("outcomes")).expect_err("duplicate");

    assert!(matches!(
        error,
        CatalogError::DuplicateDataset { name } if name.as_ref() == "outcomes"
    ));
    assert_eq!(registry.iter().count(), 1);
}

#[test]
fn report_for_undeployed_dataset_is_rejected() {
    let mut catalog = catalog();

    let error = catalog
        .insert_report(report("r", "missing"))
        .expect_err("unknown dataset");

    assert!(matches!(
        error,
        CatalogError::UnknownDataset { report, dataset }
            if report.as_ref() == "r" && dataset.as_ref() == "missing"
    ));
}

#[test]
fn incompatible_report_is_rejected_with_its_cause() {
    let mut value = report_json("r", "outcomes");
    value["variables"][0]["field"] = json!("missing");
    let incompatible: ReportDefinition = serde_json::from_value(value).expect("report");
    let mut catalog = catalog();

    let error = catalog
        .insert_report(incompatible)
        .expect_err("incompatible");

    assert!(matches!(
        error,
        CatalogError::Incompatible {
            report,
            source: IncompatibleReport::UnknownField { .. },
        } if report.as_ref() == "r"
    ));
    assert_eq!(catalog.reports().count(), 0);
}

#[test]
fn duplicate_report_is_rejected_and_catalog_is_unchanged() {
    let mut catalog = catalog();
    catalog
        .insert_report(report("r", "outcomes"))
        .expect("first insert");

    let error = catalog
        .insert_report(report("r", "other"))
        .expect_err("duplicate");

    assert!(matches!(
        error,
        CatalogError::DuplicateReport { name } if name.as_ref() == "r"
    ));
    assert_eq!(catalog.reports().count(), 1);
    assert_eq!(
        catalog.reports().next().map(|r| r.dataset().as_ref()),
        Some("outcomes")
    );
}

#[test]
fn reports_for_returns_only_that_datasets_reports_in_name_order() {
    let mut catalog = catalog();
    for (name, dataset) in [("b", "outcomes"), ("c", "other"), ("a", "outcomes")] {
        catalog
            .insert_report(report(name, dataset))
            .expect("insert");
    }
    let outcomes = DatasetName::parse("outcomes").expect("name");

    let names: Vec<_> = catalog
        .reports_for(&outcomes)
        .map(|r| r.name().as_ref())
        .collect();

    assert_eq!(names, ["a", "b"]);
}

#[test]
fn dataset_without_reports_yields_an_empty_iterator() {
    let catalog = catalog();
    let outcomes = DatasetName::parse("outcomes").expect("name");

    assert_eq!(catalog.reports_for(&outcomes).count(), 0);
}
