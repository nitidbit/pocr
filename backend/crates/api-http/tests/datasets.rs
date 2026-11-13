//! Dataset routes against a disposable PostgreSQL database named by
//! `DATASET_TEST_DATABASE_URL`, with real repositories and snapshots seeded
//! through the public refresh API.

use std::sync::Arc;

use api_http::datasets::{DatasetsState, router};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use http_body_util::BodyExt;
use infra_pg::{PgDatasetRepository, PgStudyRepository, PgUserStudyRepository};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use pocr_core::{
    dataset::{
        Catalog, ContextRecord, DatasetDefinition, DatasetName, DatasetRecord, DatasetRepository,
        ReportDefinition, ResultRow,
    },
    environment::Environment,
    ports::UserStudyRepository,
};

const OWNER: &str = "owner@example.com";
const MEMBER: &str = "member@example.com";
const OUTSIDER: &str = "outsider@example.com";
const DATASET: &str = "sample-outcomes";
const ARM: Uuid = Uuid::from_u128(100);
const EMPTY_ARM: Uuid = Uuid::from_u128(101);
const FIELD_NAMES: [&str; 13] = [
    "disposition",
    "discharge_day",
    "discharge_before_noon",
    "los",
    "admission_type",
    "admission_source",
    "discharge_department",
    "mychart_status",
    "drg_weight",
    "readmission",
    "days_to_readmission",
    "return_to_ed",
    "days_to_return_to_ed",
];

fn fixture_catalog() -> Catalog {
    infra_datasets::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../infra-datasets/tests/fixtures/catalog"
    )))
    .expect("fixture catalog loads")
}

struct Harness {
    pool: PgPool,
    repository: Arc<PgDatasetRepository>,
    members: Arc<PgUserStudyRepository>,
    study: Uuid,
    definition: DatasetDefinition,
}

impl Harness {
    async fn new() -> Self {
        let url = std::env::var("DATASET_TEST_DATABASE_URL")
            .expect("explicit disposable test database required");
        let pool = infra_pg::create_pool(&url).await.unwrap();
        infra_pg::run_migrations(&pool).await.unwrap();
        let study = Uuid::now_v7();
        let arms = json!({
            "strategy": "simple",
            "arms": [
                {"id": ARM, "name": "Treatment", "ratio": 1, "locator": null},
                {"id": EMPTY_ARM, "name": "Control", "ratio": 1, "locator": null}
            ],
            "keys": [],
            "strata": []
        });
        sqlx::query("INSERT INTO studies(id,name,randomization_config) VALUES ($1,'HTTP test',$2)")
            .bind(study)
            .bind(arms)
            .execute(&pool)
            .await
            .unwrap();
        let members = Arc::new(PgUserStudyRepository::new(pool.clone()));
        members.add_user(OWNER, &study, "owner").await.unwrap();
        members.add_user(MEMBER, &study, "member").await.unwrap();
        let definition = fixture_catalog()
            .datasets()
            .get(&DatasetName::parse(DATASET).unwrap())
            .unwrap()
            .clone();
        Self {
            repository: Arc::new(PgDatasetRepository::new(pool.clone())),
            pool,
            members,
            study,
            definition,
        }
    }

    fn router_with(&self, catalog: Catalog) -> Router {
        router().with_state(DatasetsState {
            catalog: Arc::new(catalog),
            datasets: self.repository.clone(),
            members: self.members.clone(),
            studies: Arc::new(PgStudyRepository::new(self.pool.clone())),
        })
    }

    fn router(&self) -> Router {
        self.router_with(fixture_catalog())
    }

    async fn attach(&self) {
        self.repository
            .attach(self.study, &self.definition)
            .await
            .unwrap();
    }

    async fn seed_randomization(&self, environment: Environment) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO randomizations(id,study_id,unit_hash,arm,environment)
            VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(id)
        .bind(self.study)
        .bind(id.to_string())
        .bind(json!({"id": ARM, "name": "Treatment", "ratio": 1}))
        .bind(environment.as_str())
        .execute(&self.pool)
        .await
        .unwrap();
        id
    }

    /// Attach, then publish one snapshot row whose `disposition` is `disposition`.
    async fn publish(&self, environment: Environment, disposition: &str) -> Uuid {
        self.publish_under(environment, disposition, &self.definition.clone())
            .await
    }

    /// Like [`Self::publish`], capturing `definition` as the run's definition.
    async fn publish_under(
        &self,
        environment: Environment,
        disposition: &str,
        definition: &DatasetDefinition,
    ) -> Uuid {
        self.publish_with(environment, disposition, json!(4), definition)
            .await
    }

    /// Like [`Self::publish_under`], with the row's `LOS` column set to `los`.
    async fn publish_with(
        &self,
        environment: Environment,
        disposition: &str,
        los: Value,
        definition: &DatasetDefinition,
    ) -> Uuid {
        self.attach().await;
        let randomization = self.seed_randomization(environment).await;
        let attachment = self.repository.list(self.study, environment).await.unwrap()[0].id;
        let mut run = self
            .repository
            .try_lock(attachment, environment)
            .await
            .unwrap()
            .expect("lock is free")
            .begin(definition)
            .await
            .unwrap();
        let page = run.next_page().await.unwrap();
        assert_eq!(page.len(), 1);
        run.stage(&[record(definition, randomization, disposition, los)])
            .await
            .unwrap();
        run.publish().await.unwrap();
        randomization
    }

    fn datasets_uri(&self, environment: &str) -> String {
        format!("/studies/{}/datasets?environment={environment}", self.study)
    }

    fn download_uri(&self, environment: &str) -> String {
        format!(
            "/studies/{}/datasets/{DATASET}/download?environment={environment}",
            self.study
        )
    }

    fn reports_uri(&self, environment: &str) -> String {
        format!(
            "/studies/{}/dataset-reports?environment={environment}",
            self.study
        )
    }

    fn attach_uri(&self) -> String {
        format!("/studies/{}/datasets", self.study)
    }

    fn detach_uri(&self) -> String {
        format!("/studies/{}/datasets/{DATASET}", self.study)
    }
}

fn record(
    definition: &DatasetDefinition,
    id: Uuid,
    disposition: &str,
    los: Value,
) -> DatasetRecord {
    let mut row: ResultRow = definition
        .fields()
        .iter()
        .map(|field| (field.column().as_ref().to_owned(), Value::Null))
        .collect();
    row.insert("randomization_id".into(), json!(id));
    row.insert("DISPOSITION_DESC".into(), json!(disposition));
    row.insert("LOS".into(), los);
    let input = ContextRecord {
        randomization_id: id,
        randomization_time: "2026-10-01T14:30:00Z".parse().unwrap(),
        context: Default::default(),
    };
    definition
        .parse_results(&[input], vec![row])
        .unwrap()
        .remove(0)
}

/// The fixture datasets with `reports` as the only deployed reports.
fn catalog_with_reports(reports: &[Value]) -> Catalog {
    let mut catalog = Catalog::new(fixture_catalog().datasets().clone());
    for report in reports {
        let report: ReportDefinition = serde_json::from_value(report.clone()).unwrap();
        catalog.insert_report(report).unwrap();
    }
    catalog
}

/// A report of `los` for the fixture dataset.
fn los_report(name: &str, label: &str) -> Value {
    json!({
        "version": 1,
        "name": name,
        "dataset": DATASET,
        "title": format!("{name} title"),
        "description": "",
        "variables": [{"field": "los", "label": label, "analysis": "continuous"}]
    })
}

/// The fixture definition with field `los` edited by `edit`.
fn definition_with_los(h: &Harness, edit: impl FnOnce(&mut Value)) -> DatasetDefinition {
    let mut value = serde_json::to_value(&h.definition).unwrap();
    let los = value["fields"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|field| field["name"] == "los")
        .unwrap();
    edit(los);
    serde_json::from_value(value).unwrap()
}

async fn send(
    router: Router,
    method: Method,
    uri: &str,
    user: &str,
    body: Option<Value>,
) -> (StatusCode, String) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("X-Vouch-User", user);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

async fn get(h: &Harness, uri: &str, user: &str) -> (StatusCode, String) {
    send(h.router(), Method::GET, uri, user, None).await
}

fn attach_body() -> Option<Value> {
    Some(json!({"name": DATASET}))
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn non_member_gets_not_found_listing_datasets() {
    let h = Harness::new().await;

    let (status, _) = get(&h, &h.datasets_uri("production"), OUTSIDER).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn non_member_gets_not_found_attaching_a_dataset() {
    let h = Harness::new().await;

    let (status, _) = send(
        h.router(),
        Method::POST,
        &h.attach_uri(),
        OUTSIDER,
        attach_body(),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn non_member_gets_not_found_detaching_a_dataset() {
    let h = Harness::new().await;
    h.attach().await;

    let (status, _) = send(h.router(), Method::DELETE, &h.detach_uri(), OUTSIDER, None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn non_member_gets_not_found_downloading_a_dataset() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (status, _) = get(&h, &h.download_uri("production"), OUTSIDER).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn non_member_gets_not_found_reading_reports() {
    let h = Harness::new().await;

    let (status, _) = get(&h, &h.reports_uri("production"), OUTSIDER).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn member_lists_datasets_without_manage_permission() {
    let h = Harness::new().await;
    h.attach().await;

    let (status, body) = get(&h, &h.datasets_uri("production"), MEMBER).await;

    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["canManage"], json!(false));
    assert_eq!(list["attachments"][0]["name"], json!(DATASET));
    assert_eq!(list["catalog"][0]["name"], json!(DATASET));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn member_downloads_a_published_dataset() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (status, _) = get(&h, &h.download_uri("production"), MEMBER).await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn member_reads_reports() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (status, body) = get(&h, &h.reports_uri("production"), MEMBER).await;

    let reports: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reports[0]["name"], json!(DATASET));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn report_has_a_column_for_every_configured_arm() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (_, body) = get(&h, &h.reports_uri("production"), MEMBER).await;

    let reports: Value = serde_json::from_str(&body).unwrap();
    let arms = &reports[0]["report"]["arms"];
    assert_eq!(arms[0]["name"], json!("Treatment"));
    assert_eq!(arms[0]["count"], json!(1));
    assert_eq!(arms[1]["name"], json!("Control"));
    assert_eq!(arms[1]["count"], json!(0));
    assert_eq!(arms[1]["id"], json!(EMPTY_ARM.to_string()));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn member_cannot_attach_a_dataset() {
    let h = Harness::new().await;

    let (status, _) = send(
        h.router(),
        Method::POST,
        &h.attach_uri(),
        MEMBER,
        attach_body(),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn member_cannot_detach_a_dataset() {
    let h = Harness::new().await;
    h.attach().await;

    let (status, _) = send(h.router(), Method::DELETE, &h.detach_uri(), MEMBER, None).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn owner_attaches_a_dataset_and_can_manage() {
    let h = Harness::new().await;

    let (status, _) = send(
        h.router(),
        Method::POST,
        &h.attach_uri(),
        OWNER,
        attach_body(),
    )
    .await;
    let (_, body) = get(&h, &h.datasets_uri("production"), OWNER).await;

    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(list["canManage"], json!(true));
    assert_eq!(list["attachments"][0]["name"], json!(DATASET));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn owner_detaches_a_dataset() {
    let h = Harness::new().await;
    h.attach().await;

    let (status, _) = send(h.router(), Method::DELETE, &h.detach_uri(), OWNER, None).await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    let remaining = h
        .repository
        .list(h.study, Environment::Production)
        .await
        .unwrap();
    assert!(remaining.is_empty());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attaching_an_unknown_dataset_is_a_bad_request() {
    let h = Harness::new().await;

    let (status, _) = send(
        h.router(),
        Method::POST,
        &h.attach_uri(),
        OWNER,
        Some(json!({"name": "not-deployed"})),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn production_snapshot_is_not_downloadable_from_nonproduction() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (status, _) = get(&h, &h.download_uri("nonproduction"), MEMBER).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn production_snapshot_has_no_nonproduction_report() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (status, body) = get(&h, &h.reports_uri("nonproduction"), MEMBER).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!([]));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn missing_environment_query_is_a_bad_request() {
    let h = Harness::new().await;

    let (status, _) = get(&h, &format!("/studies/{}/datasets", h.study), MEMBER).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn download_serves_the_captured_snapshot_when_the_definition_is_undeployed() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (status, body) = send(
        h.router_with(Catalog::default()),
        Method::GET,
        &h.download_uri("production"),
        MEMBER,
        None,
    )
    .await;

    let expected: Vec<String> = ["randomization_id", "arm_id", "arm_name"]
        .into_iter()
        .chain(FIELD_NAMES)
        .map(|name| format!("\"{name}\""))
        .collect();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.split("\r\n").next().unwrap(), expected.join(","));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn list_marks_an_undeployed_definition_unavailable() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (status, body) = send(
        h.router_with(Catalog::default()),
        Method::GET,
        &h.datasets_uri("production"),
        MEMBER,
        None,
    )
    .await;

    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["attachments"][0]["definitionAvailable"], json!(false));
    assert_eq!(list["attachments"][0]["title"], json!("Sample Outcomes"));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn csv_header_lists_system_columns_then_declared_fields() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (_, body) = get(&h, &h.download_uri("production"), MEMBER).await;

    let expected: Vec<String> = ["randomization_id", "arm_id", "arm_name"]
        .into_iter()
        .chain(FIELD_NAMES)
        .map(|name| format!("\"{name}\""))
        .collect();
    assert_eq!(body.split("\r\n").next().unwrap(), expected.join(","));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn csv_row_carries_ids_and_escapes_values() {
    let h = Harness::new().await;
    let randomization = h
        .publish(Environment::Production, "=SUM(1)\n\"quoted\"")
        .await;

    let (_, body) = get(&h, &h.download_uri("production"), MEMBER).await;

    let leading = format!("\"{randomization}\",\"{ARM}\",\"Treatment\"");
    let disposition = "\"'=SUM(1)\n\"\"quoted\"\"\"";
    let trailing = ["\"\"", "\"\"", "\"4\""]
        .into_iter()
        .chain(std::iter::repeat_n("\"\"", 9))
        .collect::<Vec<_>>()
        .join(",");
    let expected_row = format!("{leading},{disposition},{trailing}\r\n");
    assert!(body.ends_with(&expected_row), "{body}");
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attaching_a_malformed_dataset_name_is_unprocessable() {
    let h = Harness::new().await;

    let (status, _) = send(
        h.router(),
        Method::POST,
        &h.attach_uri(),
        OWNER,
        Some(json!({"name": "Bad Name"})),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attachment_without_a_snapshot_is_not_reported_as_changed() {
    let h = Harness::new().await;
    h.attach().await;

    let (_, body) = get(&h, &h.datasets_uri("production"), MEMBER).await;

    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(list["attachments"][0]["hasSnapshot"], json!(false));
    assert_eq!(list["attachments"][0]["definitionChanged"], json!(false));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attachment_is_reported_as_changed_when_the_deployed_definition_differs() {
    let h = Harness::new().await;
    let mut older = serde_json::to_value(&h.definition).unwrap();
    older["title"] = json!("Older title");
    let older: DatasetDefinition = serde_json::from_value(older).unwrap();
    h.publish_under(Environment::Production, "Home", &older)
        .await;

    let (_, body) = get(&h, &h.datasets_uri("production"), MEMBER).await;

    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(list["attachments"][0]["definitionChanged"], json!(true));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attachment_matching_the_deployed_definition_is_not_reported_as_changed() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;

    let (_, body) = get(&h, &h.datasets_uri("production"), MEMBER).await;

    let list: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(list["attachments"][0]["definitionChanged"], json!(false));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn report_is_incompatible_when_captured_definition_lacks_a_field() {
    let h = Harness::new().await;
    let captured = definition_with_los(&h, |los| los["name"] = json!("length_of_stay"));
    h.publish_under(Environment::Production, "Home", &captured)
        .await;
    let router = h.router_with(catalog_with_reports(&[los_report("los-report", "LOS")]));

    let (status, body) = send(
        router,
        Method::GET,
        &h.reports_uri("production"),
        MEMBER,
        None,
    )
    .await;

    let reports: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reports[0]["state"], json!("incompatible"));
    assert_eq!(reports[0]["incompatibility"]["field"], json!("los"));
    assert_eq!(
        reports[0]["incompatibility"]["reason"],
        json!("unknownField")
    );
    assert!(reports[0].get("report").is_none());
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn report_is_incompatible_when_captured_field_type_changed() {
    let h = Harness::new().await;
    let captured = definition_with_los(&h, |los| los["type"] = json!("string"));
    h.publish_with(Environment::Production, "Home", Value::Null, &captured)
        .await;
    let router = h.router_with(catalog_with_reports(&[los_report("los-report", "LOS")]));

    let (_, body) = send(
        router,
        Method::GET,
        &h.reports_uri("production"),
        MEMBER,
        None,
    )
    .await;

    let reports: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(reports[0]["state"], json!("incompatible"));
    assert_eq!(
        reports[0]["incompatibility"]["reason"],
        json!("incompatibleType")
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn report_edit_renders_without_refresh() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;
    let router = h.router_with(catalog_with_reports(&[los_report(
        "los-report",
        "Edited label",
    )]));

    let (_, body) = send(
        router,
        Method::GET,
        &h.reports_uri("production"),
        MEMBER,
        None,
    )
    .await;

    let reports: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(reports[0]["state"], json!("ready"));
    assert_eq!(
        reports[0]["report"]["fields"][0]["name"],
        json!("Edited label")
    );
    assert_eq!(reports[0]["definitionChanged"], json!(false));
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn dataset_with_two_reports_returns_both_in_name_order() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;
    let router = h.router_with(catalog_with_reports(&[
        los_report("b-report", "LOS"),
        los_report("a-report", "LOS"),
    ]));

    let (_, body) = send(
        router,
        Method::GET,
        &h.reports_uri("production"),
        MEMBER,
        None,
    )
    .await;

    let reports: Value = serde_json::from_str(&body).unwrap();
    let names: Vec<_> = reports
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["name"].clone(), entry["dataset"].clone()))
        .collect();
    assert_eq!(
        names,
        [
            (json!("a-report"), json!(DATASET)),
            (json!("b-report"), json!(DATASET))
        ]
    );
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL via DATASET_TEST_DATABASE_URL"]
async fn attached_dataset_without_a_deployed_report_is_omitted() {
    let h = Harness::new().await;
    h.publish(Environment::Production, "Home").await;
    let router = h.router_with(catalog_with_reports(&[]));

    let (status, body) = send(
        router,
        Method::GET,
        &h.reports_uri("production"),
        MEMBER,
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap(), json!([]));
}
