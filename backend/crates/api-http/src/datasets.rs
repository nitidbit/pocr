//! Study dataset attachments, downloads, and reports.
//!
//! Members can list, download, and read reports; owners also attach and
//! detach. Handlers read [`DatasetsState`] only, so they can be exercised
//! with the repositories alone.

mod export;

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{FromRef, Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{delete, get},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use pocr_core::{
    dataset::{
        Attachment, Catalog, DatasetDefinition, DatasetName, DatasetRepository, DatasetStatus,
        FieldName, IncompatibilityKind, Report, ReportName,
    },
    environment::Environment,
    ports::{RepositoryError, StudyRepository, UserStudyRepository},
    random::Arm,
};

use crate::{
    error::ApiError,
    extractors::AuthUser,
    members::{Role, caller_role},
};

/// Dependencies of the dataset handlers.
///
/// Cloned per request, so every field is cheap to clone.
#[derive(Clone)]
pub struct DatasetsState {
    /// Deployed datasets and the reports bound to them.
    pub catalog: Arc<Catalog>,
    /// Attachments and published snapshots.
    pub datasets: Arc<dyn DatasetRepository>,
    /// Study membership, used for authorization.
    pub members: Arc<dyn UserStudyRepository>,
    /// Study lookup.
    pub studies: Arc<dyn StudyRepository>,
}

/// Build the dataset routes under `/studies/{study_id}`.
///
/// Merge the result into the authenticated API router of any state that
/// yields a [`DatasetsState`]; every handler authorizes the caller through
/// [`DatasetsState::members`].
pub fn router<S>() -> Router<S>
where
    DatasetsState: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/studies/{study_id}/datasets", get(list).post(attach))
        .route("/studies/{study_id}/datasets/{name}", delete(detach))
        .route(
            "/studies/{study_id}/datasets/{name}/download",
            get(download),
        )
        .route("/studies/{study_id}/dataset-reports", get(reports))
}

/// Query string selecting the environment whose snapshots are read.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub environment: Environment,
}

/// Request body for attaching a dataset.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachRequest {
    /// Name of a deployed dataset.
    pub name: DatasetName,
}

/// An attachment together with how it compares to the deployed definition.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetAttachment {
    id: Uuid,
    study_id: Uuid,
    name: DatasetName,
    /// Deployed title, else the latest published run's, else the name.
    title: String,
    /// Deployed description, else the latest published run's; may be empty.
    description: String,
    status: DatasetStatus,
    /// When the latest published run completed; null without a snapshot.
    completed_at: Option<DateTime<Utc>>,
    /// Rows in the latest published snapshot; zero without one.
    row_count: u64,
    /// Fingerprint of the latest published snapshot's definition.
    fingerprint: Option<String>,
    has_snapshot: bool,
    /// Whether the dataset is still deployed in the catalog.
    definition_available: bool,
    /// Whether the deployed definition's fingerprint differs from the
    /// published snapshot's. False when there is no snapshot or no deployed
    /// definition to compare.
    definition_changed: bool,
}

/// A deployed dataset that can be attached.
#[derive(Serialize)]
pub struct CatalogItem {
    name: String,
    title: String,
    description: String,
}

/// Response of the list route: the deployed catalog and the study's attachments.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetList {
    catalog: Vec<CatalogItem>,
    attachments: Vec<DatasetAttachment>,
    /// Whether the caller may attach and detach datasets (study owners).
    can_manage: bool,
}

/// A deployed report rendered against one attachment's latest published snapshot.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedReport {
    /// Report name; unique across the catalog.
    name: ReportName,
    /// Dataset the report summarizes.
    dataset: DatasetName,
    /// Run that produced the summarized snapshot.
    snapshot_id: Uuid,
    status: DatasetStatus,
    completed_at: DateTime<Utc>,
    /// Whether the deployed dataset definition differs from the snapshot's.
    definition_changed: bool,
    #[serde(flatten)]
    rendering: Rendering,
}

/// Whether a report could be rendered against the snapshot's definition.
#[derive(Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
enum Rendering {
    /// The report bound; `report` summarizes the snapshot rows.
    Ready { report: Report },
    /// The captured definition no longer fits the report; no data is returned.
    Incompatible {
        title: String,
        incompatibility: Incompatibility,
    },
}

/// The dataset field that broke a report's binding, and how.
#[derive(Serialize)]
struct Incompatibility {
    field: FieldName,
    reason: IncompatibilityKind,
}

/// Overlay deployed title and description on an attachment and flag drift.
fn describe(attachment: Attachment, current: Option<&DatasetDefinition>) -> DatasetAttachment {
    let definition_changed = match (&attachment.snapshot, current) {
        (Some(published), Some(deployed)) => deployed.fingerprint() != published.fingerprint,
        _ => false,
    };
    let (title, description) = match current {
        Some(definition) => (
            definition.title().to_owned(),
            definition.description().to_owned(),
        ),
        None => (attachment.title, attachment.description),
    };
    let snapshot = attachment.snapshot;
    DatasetAttachment {
        id: attachment.id,
        study_id: attachment.study_id,
        name: attachment.name,
        title,
        description,
        status: attachment.status,
        completed_at: snapshot.as_ref().map(|published| published.completed_at),
        row_count: snapshot.as_ref().map_or(0, |published| published.row_count),
        has_snapshot: snapshot.is_some(),
        fingerprint: snapshot.map(|published| published.fingerprint),
        definition_available: current.is_some(),
        definition_changed,
    }
}

/// Allow only study owners; non-members get not found, members forbidden.
async fn require_owner(
    state: &DatasetsState,
    study: &Uuid,
    user: &AuthUser,
) -> Result<(), ApiError> {
    let role = caller_role(state.members.as_ref(), study, user).await?;
    if role >= Role::Owner {
        Ok(())
    } else {
        Err(ApiError::Forbidden(
            "only owners can manage datasets".to_owned(),
        ))
    }
}

/// `GET /studies/{study_id}/datasets` — list the catalog and the study's
/// attachments in one environment. Any member.
#[instrument(skip_all, fields(study_id = %study))]
pub async fn list(
    State(state): State<DatasetsState>,
    Path(study): Path<Uuid>,
    Query(scope): Query<Scope>,
    user: AuthUser,
) -> Result<Json<DatasetList>, ApiError> {
    let role = caller_role(state.members.as_ref(), &study, &user).await?;
    let attachments = state
        .datasets
        .list(study, scope.environment)
        .await?
        .into_iter()
        .map(|attachment| {
            let current = state.catalog.datasets().get(&attachment.name);
            describe(attachment, current)
        })
        .collect();
    let catalog = state
        .catalog
        .datasets()
        .iter()
        .map(|definition| CatalogItem {
            name: definition.name().to_string(),
            title: definition.title().to_owned(),
            description: definition.description().to_owned(),
        })
        .collect();
    Ok(Json(DatasetList {
        catalog,
        attachments,
        can_manage: role >= Role::Owner,
    }))
}

/// `POST /studies/{study_id}/datasets` — attach a deployed dataset to the
/// study in both environments. Owners only.
///
/// A well-formed name that is not deployed is rejected with 400. A name that is
/// not a valid dataset identifier fails body extraction and is rejected with 422,
/// as for any malformed JSON body.
#[instrument(skip_all, fields(study_id = %study, dataset_name = %body.name))]
pub async fn attach(
    State(state): State<DatasetsState>,
    Path(study): Path<Uuid>,
    user: AuthUser,
    Json(body): Json<AttachRequest>,
) -> Result<StatusCode, ApiError> {
    require_owner(&state, &study, &user).await?;
    let definition = state
        .catalog
        .datasets()
        .get(&body.name)
        .ok_or_else(|| ApiError::BadRequest("unknown dataset".into()))?;
    state.datasets.attach(study, definition).await?;
    info!("dataset attached");
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /studies/{study_id}/datasets/{name}` — detach a dataset and delete
/// its snapshots in both environments. Owners only.
#[instrument(skip_all, fields(study_id = %study, dataset_name = %name))]
pub async fn detach(
    State(state): State<DatasetsState>,
    Path((study, name)): Path<(Uuid, DatasetName)>,
    user: AuthUser,
) -> Result<StatusCode, ApiError> {
    require_owner(&state, &study, &user).await?;
    state.datasets.detach(study, &name).await?;
    info!("dataset detached");
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /studies/{study_id}/datasets/{name}/download` — the latest published
/// snapshot as CSV, even if the definition is no longer deployed. Any member.
#[instrument(skip_all, fields(study_id = %study, dataset_name = %name))]
pub async fn download(
    State(state): State<DatasetsState>,
    Path((study, name)): Path<(Uuid, DatasetName)>,
    Query(scope): Query<Scope>,
    user: AuthUser,
) -> Result<impl IntoResponse, ApiError> {
    caller_role(state.members.as_ref(), &study, &user).await?;
    let snapshot = state
        .datasets
        .snapshot(study, &name, scope.environment)
        .await?
        .ok_or(ApiError::NotFound)?;
    let disposition = format!("attachment; filename=\"{name}-{}.csv\"", scope.environment);
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
        export::render(&snapshot).map_err(|error| ApiError::Internal(error.to_string()))?,
    ))
}

/// Renders every deployed report of each attachment that has a published
/// snapshot. Attachments without deployed reports are skipped without reading
/// their snapshots.
async fn published_reports(
    datasets: &dyn DatasetRepository,
    catalog: &Catalog,
    study: Uuid,
    environment: Environment,
    arms: &[Arm],
) -> Result<Vec<PublishedReport>, RepositoryError> {
    let mut reports = Vec::new();
    for attachment in datasets.list(study, environment).await? {
        if catalog.reports_for(&attachment.name).next().is_none() {
            continue;
        }
        let snapshot = datasets
            .snapshot(study, &attachment.name, environment)
            .await?;
        let Some(snapshot) = snapshot else { continue };
        let definition_changed = catalog
            .datasets()
            .get(&attachment.name)
            .is_some_and(|deployed| deployed.fingerprint() != snapshot.fingerprint());
        for report in catalog.reports_for(&attachment.name) {
            let rendering = match report.bind(&snapshot.definition) {
                Ok(bound) => Rendering::Ready {
                    report: bound.summarize(arms, &snapshot.rows),
                },
                Err(error) => {
                    warn!(
                        report_name = %report.name(),
                        dataset_name = %attachment.name,
                        field = %error.field(),
                        error_message = %error,
                        "report incompatible with snapshot definition"
                    );
                    Rendering::Incompatible {
                        title: report.title().to_owned(),
                        incompatibility: Incompatibility {
                            field: error.field().clone(),
                            reason: error.kind(),
                        },
                    }
                }
            };
            reports.push(PublishedReport {
                name: report.name().clone(),
                dataset: attachment.name.clone(),
                snapshot_id: snapshot.run_id,
                status: snapshot.status,
                completed_at: snapshot.completed_at,
                definition_changed,
                rendering,
            });
        }
    }
    Ok(reports)
}

/// `GET /studies/{study_id}/dataset-reports` — summaries of every deployed
/// report whose dataset is attached and has a published snapshot, rendered
/// against the snapshot's captured definition. Any member.
///
/// A report that no longer binds to the captured definition is returned in the
/// `incompatible` state without summary data.
#[instrument(skip_all, fields(study_id = %study))]
pub async fn reports(
    State(state): State<DatasetsState>,
    Path(study): Path<Uuid>,
    Query(scope): Query<Scope>,
    user: AuthUser,
) -> Result<impl IntoResponse, ApiError> {
    caller_role(state.members.as_ref(), &study, &user).await?;
    let arms = state
        .studies
        .find_by_id(&study)
        .await?
        .ok_or(ApiError::NotFound)?
        .randomization_config
        .arms;
    let reports = published_reports(
        state.datasets.as_ref(),
        &state.catalog,
        study,
        scope.environment,
        &arms,
    )
    .await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(reports)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use pocr_core::dataset::{DatasetRegistry, PublishedSummary, ReportDefinition, Snapshot};
    use serde_json::json;

    #[test]
    fn explicit_environment_is_required() {
        assert!(serde_json::from_value::<Scope>(json!({})).is_err());
        assert!(serde_json::from_value::<Scope>(json!({"environment":"invalid"})).is_err());
    }

    #[test]
    fn attach_request_rejects_invalid_dataset_name() {
        assert!(serde_json::from_value::<AttachRequest>(json!({"name": "Bad Name"})).is_err());
    }

    /// Lists fixed attachments and counts snapshot reads.
    struct CountingRepository {
        attachments: Vec<Attachment>,
        snapshot_reads: AtomicUsize,
    }

    #[async_trait]
    impl DatasetRepository for CountingRepository {
        async fn list(&self, _: Uuid, _: Environment) -> Result<Vec<Attachment>, RepositoryError> {
            Ok(self.attachments.clone())
        }

        async fn attach(&self, _: Uuid, _: &DatasetDefinition) -> Result<(), RepositoryError> {
            unreachable!("reports never attach")
        }

        async fn detach(&self, _: Uuid, _: &DatasetName) -> Result<(), RepositoryError> {
            unreachable!("reports never detach")
        }

        async fn snapshot(
            &self,
            _: Uuid,
            _: &DatasetName,
            _: Environment,
        ) -> Result<Option<Snapshot>, RepositoryError> {
            self.snapshot_reads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
    }

    #[tokio::test]
    async fn attachment_without_deployed_reports_skips_snapshot_read() {
        let repository = CountingRepository {
            attachments: vec![attachment(None)],
            snapshot_reads: AtomicUsize::new(0),
        };
        let catalog = Catalog::new(DatasetRegistry::default());

        let reports = published_reports(
            &repository,
            &catalog,
            Uuid::from_u128(2),
            Environment::Production,
            &[],
        )
        .await
        .expect("reports load");

        assert!(reports.is_empty());
        assert_eq!(repository.snapshot_reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn attachment_with_deployed_reports_reads_snapshot() {
        let catalog = infra_datasets::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../infra-datasets/tests/fixtures/catalog"
        )))
        .expect("fixture catalog loads");
        let dataset = catalog
            .datasets()
            .iter()
            .find(|definition| catalog.reports_for(definition.name()).next().is_some())
            .expect("fixture catalog has a dataset with a report")
            .name()
            .clone();
        let repository = CountingRepository {
            attachments: vec![Attachment {
                name: dataset,
                ..attachment(None)
            }],
            snapshot_reads: AtomicUsize::new(0),
        };

        let reports = published_reports(
            &repository,
            &catalog,
            Uuid::from_u128(2),
            Environment::Production,
            &[],
        )
        .await
        .expect("reports load");

        assert!(reports.is_empty());
        assert_eq!(repository.snapshot_reads.load(Ordering::SeqCst), 1);
    }

    fn attachment(snapshot: Option<PublishedSummary>) -> Attachment {
        Attachment {
            id: Uuid::from_u128(1),
            study_id: Uuid::from_u128(2),
            name: DatasetName::parse("outcomes").expect("valid name"),
            title: "Outcomes".into(),
            description: String::new(),
            status: DatasetStatus::Succeeded,
            snapshot,
        }
    }

    #[test]
    fn attachment_serializes_published_snapshot_as_flat_camel_case_fields() {
        let published = PublishedSummary {
            completed_at: "2026-10-01T14:30:00Z".parse().expect("fixed timestamp"),
            row_count: 3,
            fingerprint: "abc".into(),
        };

        let encoded =
            serde_json::to_value(describe(attachment(Some(published)), None)).expect("serializes");

        assert_eq!(
            encoded,
            json!({
                "id": Uuid::from_u128(1),
                "studyId": Uuid::from_u128(2),
                "name": "outcomes",
                "title": "Outcomes",
                "description": "",
                "status": "succeeded",
                "completedAt": "2026-10-01T14:30:00Z",
                "rowCount": 3,
                "fingerprint": "abc",
                "hasSnapshot": true,
                "definitionAvailable": false,
                "definitionChanged": false
            })
        );
    }

    #[test]
    fn attachment_without_snapshot_serializes_empty_snapshot_fields() {
        let encoded = serde_json::to_value(describe(attachment(None), None)).expect("serializes");

        assert_eq!(encoded["completedAt"], json!(null));
        assert_eq!(encoded["rowCount"], json!(0));
        assert_eq!(encoded["fingerprint"], json!(null));
        assert_eq!(encoded["hasSnapshot"], json!(false));
    }

    fn published(rendering: Rendering) -> serde_json::Value {
        serde_json::to_value(PublishedReport {
            name: ReportName::parse("readmissions").expect("valid name"),
            dataset: DatasetName::parse("outcomes").expect("valid name"),
            snapshot_id: Uuid::from_u128(3),
            status: DatasetStatus::Succeeded,
            completed_at: "2026-10-01T14:30:00Z".parse().expect("fixed timestamp"),
            definition_changed: true,
            rendering,
        })
        .expect("serializes")
    }

    fn incompatible(reason: IncompatibilityKind) -> Rendering {
        Rendering::Incompatible {
            title: "Readmissions".into(),
            incompatibility: Incompatibility {
                field: FieldName::parse("los").expect("valid name"),
                reason,
            },
        }
    }

    fn ready() -> Rendering {
        let dataset: DatasetDefinition = serde_json::from_value(json!({
            "version": 1, "name": "outcomes", "title": "Outcomes", "source": "caboodle",
            "stored_procedure": {"schema": "pocr", "name": "Outcomes"},
            "fields": [{"name": "los", "type": "number", "column": "LOS"}]
        }))
        .expect("valid dataset");
        let report: ReportDefinition = serde_json::from_value(json!({
            "version": 1, "name": "readmissions", "dataset": "outcomes",
            "title": "Readmissions", "description": "By arm",
            "variables": [{"field": "los", "label": "LOS", "analysis": "continuous"}]
        }))
        .expect("valid report");
        let summary = report.bind(&dataset).expect("binds").summarize(&[], &[]);
        Rendering::Ready { report: summary }
    }

    #[test]
    fn ready_report_serializes_flat_with_state_and_report() {
        let Rendering::Ready { report } = ready() else {
            unreachable!("ready() builds a ready rendering");
        };
        let expected_report = serde_json::to_value(&report).expect("serializes");

        let encoded = published(Rendering::Ready { report });

        assert_eq!(
            encoded,
            json!({
                "name": "readmissions",
                "dataset": "outcomes",
                "snapshotId": Uuid::from_u128(3),
                "status": "succeeded",
                "completedAt": "2026-10-01T14:30:00Z",
                "definitionChanged": true,
                "state": "ready",
                "report": expected_report
            })
        );
        assert_eq!(encoded["report"]["title"], json!("Readmissions"));
    }

    #[test]
    fn incompatible_report_serializes_without_report_data() {
        let encoded = published(incompatible(IncompatibilityKind::UnknownField));

        assert_eq!(
            encoded,
            json!({
                "name": "readmissions",
                "dataset": "outcomes",
                "snapshotId": Uuid::from_u128(3),
                "status": "succeeded",
                "completedAt": "2026-10-01T14:30:00Z",
                "definitionChanged": true,
                "state": "incompatible",
                "title": "Readmissions",
                "incompatibility": {"field": "los", "reason": "unknownField"}
            })
        );
    }

    #[test]
    fn incompatible_type_reason_serializes_in_camel_case() {
        let encoded = published(incompatible(IncompatibilityKind::IncompatibleType));

        assert_eq!(
            encoded["incompatibility"]["reason"],
            json!("incompatibleType")
        );
    }
}
