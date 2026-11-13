//! Catalog loading from dataset and report package directories.

use std::fs;
use std::path::Path;

use infra_datasets::{LoadError, load};
use pocr_core::dataset::{CatalogError, DatasetName, ReportDefinition, ReportName};

const DATASET_YAML: &str = "version: 1
name: sample
title: Sample
source: caboodle
stored_procedure:
  schema: pocr
  name: Sample
fields:
  - name: los
    type: number
    column: LOS
";

const REPORT_YAML: &str = "version: 1
name: sample-report
dataset: sample
title: Report
description: ''
variables:
  - field: los
    label: LOS
    analysis: continuous
";

fn catalog(packages: &[(&str, &[(&str, &str)])]) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp directory");
    for (directory, files) in packages {
        let package = root.path().join(directory);
        fs::create_dir_all(&package).expect("package directory");
        for (name, contents) in *files {
            fs::write(package.join(name), contents).expect("package file");
        }
    }
    root
}

fn sample_with_report() -> tempfile::TempDir {
    catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        ("reports/sample-report", &[("report.yaml", REPORT_YAML)]),
    ])
}

#[test]
fn load_accepts_shipped_catalog() {
    load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../datasets"
    )))
    .expect("shipped catalog loads");
}

fn fixture_catalog() -> pocr_core::dataset::Catalog {
    load(Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/catalog"
    )))
    .expect("fixture catalog loads")
}

#[test]
fn load_preserves_field_order_from_dataset_file() {
    let catalog = fixture_catalog();
    let dataset = DatasetName::parse("sample-outcomes").expect("name");

    let definition = catalog.datasets().get(&dataset).expect("fixture dataset");

    assert_eq!(definition.fields()[0].name().as_ref(), "disposition");
}

#[test]
fn load_binds_fixture_report_to_its_dataset() {
    let catalog = fixture_catalog();
    let dataset = DatasetName::parse("sample-outcomes").expect("name");
    let report = ReportName::parse("sample-outcomes").expect("name");

    assert_eq!(
        catalog.report(&report).map(ReportDefinition::dataset),
        Some(&dataset)
    );
}

#[test]
fn load_accepts_catalog_without_reports_directory() {
    let root = catalog(&[("sample", &[("dataset.yaml", DATASET_YAML)])]);

    let catalog = load(root.path()).expect("loads");

    assert_eq!(catalog.datasets().iter().count(), 1);
    assert_eq!(catalog.reports().count(), 0);
}

#[test]
fn load_binds_two_reports_to_one_dataset() {
    let second = REPORT_YAML.replace("sample-report", "second-report");
    let root = catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        ("reports/sample-report", &[("report.yaml", REPORT_YAML)]),
        ("reports/second-report", &[("report.yaml", second.as_str())]),
    ]);

    let catalog = load(root.path()).expect("loads");

    assert_eq!(catalog.reports().count(), 2);
}

#[test]
fn load_does_not_treat_reports_directory_as_a_dataset_package() {
    let root = sample_with_report();

    let catalog = load(root.path()).expect("loads");

    assert_eq!(catalog.datasets().iter().count(), 1);
}

#[test]
fn load_ignores_hidden_and_tests_directories() {
    let root = catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        (".git", &[]),
        ("tests", &[("contracts.json", "{}")]),
    ]);

    let catalog = load(root.path()).expect("loads");

    assert_eq!(catalog.datasets().iter().count(), 1);
}

#[test]
fn load_rejects_misspelled_definition_file() {
    let root = catalog(&[("sample", &[("dataset.yml", DATASET_YAML)])]);

    let error = load(root.path()).expect_err("typo is rejected");

    assert!(matches!(error, LoadError::MissingDefinition { .. }));
}

#[test]
fn load_rejects_directory_name_that_differs_from_dataset() {
    let root = catalog(&[("other", &[("dataset.yaml", DATASET_YAML)])]);

    let error = load(root.path()).expect_err("mismatch is rejected");

    assert!(matches!(
        error,
        LoadError::DirectoryNameMismatch { name, .. } if name.as_ref() == "sample"
    ));
}

#[test]
fn load_rejects_report_directory_without_report_file() {
    let root = catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        ("reports/sample-report", &[("report.yml", REPORT_YAML)]),
    ]);

    let error = load(root.path()).expect_err("typo is rejected");

    assert!(matches!(error, LoadError::MissingReport { .. }));
}

#[test]
fn load_rejects_report_directory_name_that_differs_from_report() {
    let root = catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        ("reports/other", &[("report.yaml", REPORT_YAML)]),
    ]);

    let error = load(root.path()).expect_err("mismatch is rejected");

    assert!(matches!(
        error,
        LoadError::ReportDirectoryNameMismatch { name, .. } if name.as_ref() == "sample-report"
    ));
}

#[test]
fn load_rejects_report_for_undeployed_dataset() {
    let report = REPORT_YAML.replace("dataset: sample", "dataset: retired");
    let root = catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        ("reports/sample-report", &[("report.yaml", report.as_str())]),
    ]);

    let error = load(root.path()).expect_err("unknown dataset is rejected");

    assert!(matches!(
        error,
        LoadError::Catalog {
            source: CatalogError::UnknownDataset { .. },
            ..
        }
    ));
}

#[test]
fn load_rejects_report_for_unknown_field() {
    let report = REPORT_YAML.replace("field: los", "field: missing");
    let root = catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        ("reports/sample-report", &[("report.yaml", report.as_str())]),
    ]);

    let error = load(root.path()).expect_err("unknown field is rejected");

    assert!(matches!(
        error,
        LoadError::Catalog {
            source: CatalogError::Incompatible { .. },
            ..
        }
    ));
}

#[test]
fn load_locates_yaml_error_by_path_and_position() {
    let root = catalog(&[(
        "sample",
        &[("dataset.yaml", "version: one\nname: sample\n")],
    )]);

    let error = load(root.path()).expect_err("type error is rejected");

    let LoadError::Yaml { path, source } = error else {
        panic!("expected a YAML error, got {error:?}");
    };
    assert!(path.ends_with("dataset.yaml"));
    assert_eq!(source.location().map(|location| location.line()), Some(1));
}

#[test]
fn load_rejects_duplicate_yaml_keys_in_nested_maps() {
    let duplicated = DATASET_YAML.replace("  schema: pocr\n", "  schema: pocr\n  schema: other\n");
    let root = catalog(&[("sample", &[("dataset.yaml", duplicated.as_str())])]);

    let error = load(root.path()).expect_err("duplicate key is rejected");

    assert!(matches!(error, LoadError::Yaml { .. }));
}

#[test]
fn load_rejects_contract_violation_as_yaml_error() {
    let invalid = DATASET_YAML.replace("title: Sample", "title: ' '");
    let root = catalog(&[("sample", &[("dataset.yaml", invalid.as_str())])]);

    let error = load(root.path()).expect_err("blank title is rejected");

    assert!(matches!(error, LoadError::Yaml { .. }));
}

#[test]
fn load_reports_intrinsic_report_violation_as_yaml_error() {
    let report = REPORT_YAML.replace("title: Report", "title: ' '");
    let root = catalog(&[
        ("sample", &[("dataset.yaml", DATASET_YAML)]),
        ("reports/sample-report", &[("report.yaml", report.as_str())]),
    ]);

    let error = load(root.path()).expect_err("blank title is rejected");

    assert!(matches!(error, LoadError::Yaml { .. }));
}

#[test]
fn load_fails_when_catalog_directory_is_missing() {
    let root = tempfile::tempdir().expect("temp directory");

    let error = load(&root.path().join("absent")).expect_err("missing directory");

    assert!(matches!(error, LoadError::ReadDirectory { .. }));
}
