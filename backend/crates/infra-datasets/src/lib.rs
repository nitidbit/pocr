//! Server-managed dataset catalog loading.
//!
//! A catalog directory holds dataset packages and report packages. Each
//! dataset package is a directory named after its dataset that holds
//! `dataset.yaml`. Each report package is `reports/<report-name>/report.yaml`,
//! whose `dataset` key names a deployed dataset. [`load`] reads a catalog
//! directory and [`from_env`] locates it through `DATASETS_DIR`; any invalid
//! package fails the load so startup surfaces the problem.

use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};

use pocr_core::dataset::{
    Catalog, CatalogError, DatasetDefinition, DatasetName, DatasetRegistry, ReportDefinition,
    ReportName,
};
use serde::de::DeserializeOwned;
use tracing::info;

const DATASET_FILE: &str = "dataset.yaml";
const REPORT_FILE: &str = "report.yaml";
const TESTS_DIRECTORY: &str = "tests";
const REPORTS_DIRECTORY: &str = "reports";
const DATASETS_DIR_VARIABLE: &str = "DATASETS_DIR";

/// Reasons a dataset catalog fails to load.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LoadError {
    /// The catalog directory or one of its entries cannot be read.
    #[error("cannot read directory {}", path.display())]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A definition file cannot be read.
    #[error("cannot read file {}", path.display())]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A definition file is not valid YAML for its contract: syntax, types,
    /// duplicate keys, and every contract violation that does not depend on
    /// another definition. The source carries the line and column.
    #[error("invalid YAML in {}", path.display())]
    Yaml {
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// A definition cannot join the catalog: it duplicates another, a report
    /// names an undeployed dataset, or a report does not fit its dataset.
    /// `path` is the offending `dataset.yaml` or `report.yaml`.
    #[error("cannot add {} to the catalog", path.display())]
    Catalog {
        path: PathBuf,
        #[source]
        source: CatalogError,
    },
    /// A dataset package directory has no `dataset.yaml`.
    #[error("package directory {} has no {DATASET_FILE}", directory.display())]
    MissingDefinition { directory: PathBuf },
    /// A dataset package directory name differs from the dataset it defines.
    #[error("package directory {} defines dataset {name}", directory.display())]
    DirectoryNameMismatch {
        directory: PathBuf,
        name: DatasetName,
    },
    /// A report directory has no `report.yaml`.
    #[error("report directory {} has no {REPORT_FILE}", directory.display())]
    MissingReport { directory: PathBuf },
    /// A report directory name differs from the report it defines.
    #[error("report directory {} defines report {name}", directory.display())]
    ReportDirectoryNameMismatch {
        directory: PathBuf,
        name: ReportName,
    },
    /// An environment variable is not valid Unicode.
    #[error("environment variable {variable} is not valid Unicode")]
    InvalidEnvironment { variable: &'static str },
}

impl LoadError {
    /// Render the error followed by its full source chain, separated by `: `.
    pub fn display_chain(&self) -> String {
        let mut rendered = self.to_string();
        let mut source = self.source();
        while let Some(cause) = source {
            rendered.push_str(": ");
            rendered.push_str(&cause.to_string());
            source = cause.source();
        }
        rendered
    }
}

fn read_text(path: &Path) -> Result<String, LoadError> {
    std::fs::read_to_string(path).map_err(|source| LoadError::ReadFile {
        path: path.to_owned(),
        source,
    })
}

fn parse_yaml<T: DeserializeOwned>(path: &Path) -> Result<T, LoadError> {
    let yaml_error = |source| LoadError::Yaml {
        path: path.to_owned(),
        source,
    };
    let text = read_text(path)?;
    // Parsing to a Value first rejects duplicate keys in nested maps, which
    // typed deserialization would silently collapse.
    serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text).map_err(yaml_error)?;
    serde_yaml_ng::from_str(&text).map_err(yaml_error)
}

/// Lists the package directories under `path` in name order, skipping hidden
/// directories, plain files, and the `reserved` names.
fn package_directories(path: &Path, reserved: &[&str]) -> Result<Vec<PathBuf>, LoadError> {
    let read_error = |source| LoadError::ReadDirectory {
        path: path.to_owned(),
        source,
    };
    let mut directories = Vec::new();
    for entry in std::fs::read_dir(path).map_err(read_error)? {
        let entry = entry.map_err(read_error)?;
        let is_directory = entry.file_type().map_err(read_error)?.is_dir();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_ignored = name.starts_with('.') || reserved.contains(&name.as_ref());
        if is_directory && !is_ignored {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

fn directory_name(directory: &Path) -> Option<&str> {
    directory.file_name().and_then(|name| name.to_str())
}

fn load_dataset(directory: &Path) -> Result<DatasetDefinition, LoadError> {
    let dataset_path = directory.join(DATASET_FILE);
    if !dataset_path.is_file() {
        return Err(LoadError::MissingDefinition {
            directory: directory.to_owned(),
        });
    }
    let dataset: DatasetDefinition = parse_yaml(&dataset_path)?;
    if directory_name(directory) != Some(dataset.name().as_ref()) {
        return Err(LoadError::DirectoryNameMismatch {
            directory: directory.to_owned(),
            name: dataset.name().clone(),
        });
    }
    Ok(dataset)
}

fn load_report(directory: &Path) -> Result<ReportDefinition, LoadError> {
    let report_path = directory.join(REPORT_FILE);
    if !report_path.is_file() {
        return Err(LoadError::MissingReport {
            directory: directory.to_owned(),
        });
    }
    let report: ReportDefinition = parse_yaml(&report_path)?;
    if directory_name(directory) != Some(report.name().as_ref()) {
        return Err(LoadError::ReportDirectoryNameMismatch {
            directory: directory.to_owned(),
            name: report.name().clone(),
        });
    }
    Ok(report)
}

/// Loads every dataset and report package under `path`.
///
/// Datasets are the directories at the root; the reserved `tests` and
/// `reports` directories, hidden directories, and plain files are ignored.
/// Reports are the directories under `reports/`, which may be absent. Every
/// report must name a deployed dataset and fit it.
///
/// # Errors
///
/// Returns a [`LoadError`] naming the offending path if a directory cannot
/// be read, a package lacks its YAML file, a file is invalid, a directory
/// name differs from its definition's name or a definition cannot join the catalog.
pub fn load(path: &Path) -> Result<Catalog, LoadError> {
    let catalog_error = |path: PathBuf| move |source| LoadError::Catalog { path, source };
    let mut datasets = DatasetRegistry::default();
    for directory in package_directories(path, &[TESTS_DIRECTORY, REPORTS_DIRECTORY])? {
        let dataset = load_dataset(&directory)?;
        datasets
            .insert(dataset)
            .map_err(catalog_error(directory.join(DATASET_FILE)))?;
    }
    let mut catalog = Catalog::new(datasets);
    let reports_path = path.join(REPORTS_DIRECTORY);
    if reports_path.is_dir() {
        for directory in package_directories(&reports_path, &[])? {
            let report = load_report(&directory)?;
            catalog
                .insert_report(report)
                .map_err(catalog_error(directory.join(REPORT_FILE)))?;
        }
    }
    info!(
        path = %path.display(),
        dataset_count = catalog.datasets().iter().count(),
        report_count = catalog.reports().count(),
        "dataset catalog loaded"
    );
    Ok(catalog)
}

/// Loads the catalog named by `DATASETS_DIR`.
///
/// Returns `Ok(None)` when the variable is unset, meaning datasets are disabled.
///
/// # Errors
///
/// Returns [`LoadError::InvalidEnvironment`] if the variable is not valid
/// Unicode, or any error from [`load`].
pub fn from_env() -> Result<Option<Catalog>, LoadError> {
    match std::env::var(DATASETS_DIR_VARIABLE) {
        Ok(path) => load(Path::new(&path)).map(Some),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(LoadError::InvalidEnvironment {
            variable: DATASETS_DIR_VARIABLE,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

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

    #[test]
    fn load_reads_fixture_catalog_in_field_order() {
        let catalog = load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/catalog"
        )))
        .expect("fixture catalog loads");
        let dataset = DatasetName::parse("sample-outcomes").expect("name");
        let report = ReportName::parse("sample-outcomes").expect("name");

        let definition = catalog.datasets().get(&dataset).expect("fixture dataset");

        assert_eq!(definition.fields()[0].name().as_ref(), "disposition");
        assert_eq!(catalog.report(&report).map(|r| r.dataset()), Some(&dataset));
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
    fn load_reports_yaml_path_and_position() {
        let root = catalog(&[(
            "sample",
            &[("dataset.yaml", "version: one\nname: sample\n")],
        )]);

        let error = load(root.path()).expect_err("type error is rejected");

        let rendered = error.display_chain();
        assert!(matches!(error, LoadError::Yaml { .. }));
        assert!(rendered.contains("dataset.yaml"), "{rendered}");
        assert!(rendered.contains("line 1"), "{rendered}");
    }

    #[test]
    fn load_rejects_duplicate_yaml_keys_in_nested_maps() {
        let duplicated =
            DATASET_YAML.replace("  schema: pocr\n", "  schema: pocr\n  schema: other\n");
        let root = catalog(&[("sample", &[("dataset.yaml", duplicated.as_str())])]);

        let error = load(root.path()).expect_err("duplicate key is rejected");

        assert!(matches!(error, LoadError::Yaml { .. }));
    }

    #[test]
    fn load_reports_contract_violation_with_cause() {
        let invalid = DATASET_YAML.replace("title: Sample", "title: ' '");
        let root = catalog(&[("sample", &[("dataset.yaml", invalid.as_str())])]);

        let error = load(root.path()).expect_err("blank title is rejected");

        assert!(matches!(error, LoadError::Yaml { .. }));
        assert!(error.display_chain().contains("title is blank"));
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
}
