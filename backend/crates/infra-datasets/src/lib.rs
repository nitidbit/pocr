//! Server-managed dataset catalog loading.
//!
//! A catalog directory holds dataset packages and report packages. Each
//! dataset package is a directory named after its dataset that holds
//! `dataset.yaml`. Each report package is `reports/<report-name>/report.yaml`,
//! whose `dataset` key names a deployed dataset. [`load`] reads a catalog
//! directory and [`from_env`] locates it through `DATASETS_DIR`; any invalid
//! package fails the load so startup surfaces the problem.

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
        /// Directory that could not be read.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// A definition file cannot be read.
    #[error("cannot read file {}", path.display())]
    ReadFile {
        /// File that could not be read.
        path: PathBuf,
        /// Underlying I/O failure.
        #[source]
        source: io::Error,
    },
    /// A definition file is not valid YAML for its contract: syntax, types,
    /// duplicate keys, and every contract violation that does not depend on
    /// another definition. The source carries the line and column.
    #[error("invalid YAML in {}", path.display())]
    Yaml {
        /// File that failed to parse.
        path: PathBuf,
        /// Parser failure.
        #[source]
        source: serde_yaml_ng::Error,
    },
    /// A definition cannot join the catalog: it duplicates another, a report
    /// names an undeployed dataset, or a report does not fit its dataset.
    /// `path` is the offending `dataset.yaml` or `report.yaml`.
    #[error("cannot add {} to the catalog", path.display())]
    Catalog {
        /// The offending `dataset.yaml` or `report.yaml`.
        path: PathBuf,
        /// Reason the definition cannot join the catalog.
        #[source]
        source: CatalogError,
    },
    /// A dataset package directory has no `dataset.yaml`.
    #[error("package directory {} has no {DATASET_FILE}", directory.display())]
    MissingDefinition {
        /// Package directory lacking `dataset.yaml`.
        directory: PathBuf,
    },
    /// A dataset package directory name differs from the dataset it defines.
    #[error("package directory {} defines dataset {name}", directory.display())]
    DirectoryNameMismatch {
        /// Package directory whose name differs.
        directory: PathBuf,
        /// Name the definition declares.
        name: DatasetName,
    },
    /// A report directory has no `report.yaml`.
    #[error("report directory {} has no {REPORT_FILE}", directory.display())]
    MissingReport {
        /// Report directory lacking `report.yaml`.
        directory: PathBuf,
    },
    /// A report directory name differs from the report it defines.
    #[error("report directory {} defines report {name}", directory.display())]
    ReportDirectoryNameMismatch {
        /// Report directory whose name differs.
        directory: PathBuf,
        /// Name the definition declares.
        name: ReportName,
    },
    /// An environment variable is not valid Unicode.
    #[error("environment variable {variable} is not valid Unicode")]
    InvalidEnvironment {
        /// Name of the offending variable.
        variable: &'static str,
    },
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
/// name differs from its definition's name, or a definition cannot join the
/// catalog.
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
