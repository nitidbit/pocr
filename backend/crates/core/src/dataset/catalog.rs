use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use super::definition::{DatasetDefinition, DatasetName};
use super::report::{IncompatibleReport, ReportDefinition, ReportName};

/// Deployed dataset definitions keyed by name.
#[derive(Debug, Clone, Default)]
pub struct DatasetRegistry(BTreeMap<DatasetName, DatasetDefinition>);

impl DatasetRegistry {
    /// Adds a definition.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::DuplicateDataset`] if the name is already
    /// registered; the registry is unchanged.
    pub fn insert(&mut self, definition: DatasetDefinition) -> Result<(), CatalogError> {
        match self.0.entry(definition.name().clone()) {
            Entry::Occupied(entry) => Err(CatalogError::DuplicateDataset {
                name: entry.key().clone(),
            }),
            Entry::Vacant(entry) => {
                entry.insert(definition);
                Ok(())
            }
        }
    }

    /// Looks up a definition by name.
    #[must_use]
    pub fn get(&self, name: &DatasetName) -> Option<&DatasetDefinition> {
        self.0.get(name)
    }

    /// Iterates definitions in name order.
    pub fn iter(&self) -> impl Iterator<Item = &DatasetDefinition> {
        self.0.values()
    }

    /// Returns whether no definitions are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Deployed datasets and the reports bound to them; every report binds to its
/// dataset.
///
/// Bound reports are not stored; callers bind on use, typically against a
/// snapshot's captured definition.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    datasets: DatasetRegistry,
    reports: BTreeMap<ReportName, ReportDefinition>,
}

impl Catalog {
    /// Creates a catalog of `datasets` with no reports.
    #[must_use]
    pub fn new(datasets: DatasetRegistry) -> Self {
        Self {
            datasets,
            reports: BTreeMap::new(),
        }
    }

    /// Adds a report after binding it to its deployed dataset.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::DuplicateReport`] if the name is taken,
    /// [`CatalogError::UnknownDataset`] if the report's dataset is not
    /// deployed, or [`CatalogError::Incompatible`] if the report does not fit
    /// that dataset, checked in that order. The catalog is unchanged on error.
    pub fn insert_report(&mut self, report: ReportDefinition) -> Result<(), CatalogError> {
        if self.reports.contains_key(report.name()) {
            return Err(CatalogError::DuplicateReport {
                name: report.name().clone(),
            });
        }
        let dataset =
            self.datasets
                .get(report.dataset())
                .ok_or_else(|| CatalogError::UnknownDataset {
                    report: report.name().clone(),
                    dataset: report.dataset().clone(),
                })?;
        report
            .bind(dataset)
            .map_err(|source| CatalogError::Incompatible {
                report: report.name().clone(),
                source,
            })?;
        self.reports.insert(report.name().clone(), report);
        Ok(())
    }

    /// The deployed datasets.
    #[must_use]
    pub fn datasets(&self) -> &DatasetRegistry {
        &self.datasets
    }

    /// Looks up a report by name.
    #[must_use]
    pub fn report(&self, name: &ReportName) -> Option<&ReportDefinition> {
        self.reports.get(name)
    }

    /// Iterates reports in report-name order.
    pub fn reports(&self) -> impl Iterator<Item = &ReportDefinition> {
        self.reports.values()
    }

    /// Iterates the reports of `dataset` in report-name order.
    pub fn reports_for(&self, dataset: &DatasetName) -> impl Iterator<Item = &ReportDefinition> {
        self.reports()
            .filter(move |report| report.dataset() == dataset)
    }
}

/// Reasons a dataset or report cannot join a catalog.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CatalogError {
    /// Two datasets share a name.
    #[error("duplicate dataset {name}")]
    DuplicateDataset {
        /// Name both datasets share.
        name: DatasetName,
    },
    /// Two reports share a name.
    #[error("duplicate report {name}")]
    DuplicateReport {
        /// Name both reports share.
        name: ReportName,
    },
    /// A report names a dataset that is not deployed.
    #[error("report {report} names undeployed dataset {dataset}")]
    UnknownDataset {
        /// Report that names the missing dataset.
        report: ReportName,
        /// Dataset name that is not deployed.
        dataset: DatasetName,
    },
    /// A report does not fit its dataset.
    #[error("report {report} is incompatible with its dataset")]
    Incompatible {
        /// Report that does not fit.
        report: ReportName,
        /// Why the report does not fit.
        #[source]
        source: IncompatibleReport,
    },
}
