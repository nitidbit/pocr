# Study datasets

A dataset is a stored-procedure query against Caboodle whose results are captured as
per-study snapshots in PostgreSQL. A report is a separate deployable unit that
references one dataset and summarizes its snapshot by study arm.

## Packages

Datasets and reports are packages: directories under `DATASETS_DIR`.

```
datasets/
  dataset.schema.json  report.schema.json  README.md
  <dataset-name>/
    dataset.yaml
  reports/
    <report-name>/
      report.yaml
  tests/                shared contract corpus; ignored by the loader
```

- A dataset directory name equals the dataset `name`; a report directory name equals
  the report `name`. Report and dataset names are separate namespaces.
- Every non-hidden directory at the root except `reports` and `tests` is a dataset
  package and contains `dataset.yaml`. `reports` and `tests` are reserved, so a
  dataset cannot be named `reports` or `tests`.
- Every directory under `reports/` contains `report.yaml`. A missing `reports/`
  directory means no reports.
- A report's `dataset` must name a deployed dataset, and the report must fit it. A
  dataset has zero or more reports.
- Engine and sync read the same `DATASETS_DIR`. Definitions load and validate at
  startup; restart both after deploying a package.
- An unset `DATASETS_DIR` disables the catalog and the refresh phase; existing
  snapshots stay readable. An invalid directory or definition fails startup with
  the file path and, for YAML errors, the line and column. A report naming an
  undeployed dataset fails startup, so retiring a dataset retires its reports.

## Definition contract (version 1)

Both files declare `version: 1`. [dataset.schema.json](dataset.schema.json) and
[report.schema.json](report.schema.json) describe the structural contract; the
Rust validation is authoritative and also checks cross-references between fields,
columns, and report variables. A shared corpus (`tests/contracts.json`) drives both
the Rust and the JSON Schema tests. Duplicate YAML keys are rejected.

### `dataset.yaml`

| Key | Rule |
|---|---|
| `name` | 1-128 characters of `[a-z0-9-]`; equals the directory name |
| `title`, `description` | `title` is not blank |
| `source` | `caboodle` |
| `stored_procedure` | `{schema, name}`, each a bare SQL identifier (`[A-Za-z_][A-Za-z0-9_]*`, at most 128 characters) |
| `fields` | ordered list of `{name, type, column}`, at least one |

Field `name` is a bare identifier and not `randomization_id`, `arm_id`, or `arm_name`.
`type` is `string`, `number`, or `boolean`; every value may be null. `column` is the
procedure's result column and is unique and non-blank. `randomization_id` is the
fixed row key, never a field. Reordering fields changes the fingerprint and the CSV
column order.

### `report.yaml`

| Key | Rule |
|---|---|
| `name` | 1-128 characters of `[a-z0-9-]`; equals the directory name |
| `dataset` | name of a deployed dataset, same character rule |
| `title`, `description` | `title` is not blank |
| `variables` | at least one; each dataset field appears at most once |

A variable has `field`, `label`, `analysis`, and optional `unit`, `decimals`, `note`,
`category_order`, and `include_when`.

- `analysis: categorical` applies to `string` and `boolean` fields. It reports n (%)
  per category. A categorical variable that declares `unit`, `decimals`, or
  `include_when` is rejected.
- `analysis: continuous` applies to `number` fields. It reports mean (sample SD)
  per arm. A continuous variable that declares `category_order` is rejected.
- `unit` is a display label, never a conversion, and is rejected when blank.
  `decimals` is 0-6 and defaults to 1.
- `include_when: {field, equals}` restricts a continuous variable to rows where
  another field equals a value of that field's type; `field` must be declared and
  `equals` must have its type. Numbers compare by value, so `1` matches `1.0`. The
  denominator is shown when it differs from the arm count.
- `category_order` lists string categories first; unlisted values follow in lexical
  order and `Missing` is last. It is rejected on boolean fields, which render
  `Yes`, `No`, `Missing`.
- Labels reach the UI verbatim.

The checks above that depend only on the report file are made when it loads. The
checks against the dataset (each `field` exists, the analysis fits the field type,
`include_when` compares with the field's type, and no category order on a boolean
field) are made against the deployed dataset at load and against each snapshot's
captured definition when the report is rendered.

In the report JSON, a category `id` is the stored value: `"true"` or `"false"` for
boolean fields, the string itself for string fields, and `null` for the missing
category.

## Procedure contract

POCR validates and brackets both identifiers, then binds the context JSON to `@P1`:

```sql
EXEC [pocr].[StudyOutcomes] @context_json = @P1;
```

The database owner deploys the procedure separately and grants the sync account
`EXECUTE` on it. The procedure declares `@context_json nvarchar(max)`, uses
`SET NOCOUNT ON`, and never executes SQL taken from context values. POCR accepts
no SQL text from definitions.

**Input.** A JSON array of `{randomization_id, randomization_time, context}`:
UUID IDs, UTC timestamps, and `context` as the stored snapshot captured at
randomization. The procedure selects the properties it needs with `OPENJSON`.

**Output.** Exactly one result set whose columns are `randomization_id` plus each
declared `column`, each once, with one row per input ID. Preserve unmatched inputs
with null clinical values. Results are correlated by ID, not by position; an empty
input returns an empty result set with the same columns.

Rejected, failing the whole run: a different column set, a repeated or unknown ID,
a different row count, more or fewer than one result set, a value that does not
match its declared type (strings are not coerced to numbers), unsupported SQL types,
and nonfinite numbers. Booleans are SQL `bit`; missing values are SQL `NULL`.
Error messages carry identifiers and counts only, never row or context values.

## Refresh

From `backend`:

```sh
DATASETS_DIR="$PWD/datasets" DATASET_ENVIRONMENT=nonproduction cargo run -p pocr-sync
```

| Variable | Used by | Meaning |
|---|---|---|
| `DATASETS_DIR` | engine, sync | package directory |
| `DATASET_ENVIRONMENT` | sync | `production` or `nonproduction`; required when `DATASETS_DIR` is set |
| `DATASET_REFRESH_CONCURRENCY` | sync | attachments refreshed at once; positive integer, defaults to 1 |
| `DATASET_PROCEDURE_TIMEOUT_SECS` | sync | time limit per procedure call in seconds; positive integer, defaults to 300 |
| `DATABASE_URL` | engine, sync | PostgreSQL |
| `CABOODLE_HOST`, `CABOODLE_DATABASE`, `CABOODLE_USERNAME`, `CABOODLE_PASSWORD` | sync | SQL Server connection |
| `CABOODLE_PORT` | sync | defaults to 1433 |

Sync runs the dataset phase, then the demographics phase. There is no UI refresh
action and no in-process scheduler; invoke the job from the deployment's scheduler.

Every run refreshes the full captured cohort: all assignments of the attached study
in the chosen environment at the moment the run begins. The cohort is read in
keyset pages and split into batches of at most 1,000 records and 1 MiB of encoded
JSON. A single oversized context fails the run. Up to `DATASET_REFRESH_CONCURRENCY`
attachments refresh at once, each holding one Caboodle connection and one dedicated
PostgreSQL connection, so size the Caboodle workload and the PostgreSQL server's
connection limit for that many. Within a run, each procedure call times out after
`DATASET_PROCEDURE_TIMEOUT_SECS`, and any error abandons the run and drops the
connection.

Per attachment and environment:

- A session advisory lock serializes refreshes. A second job skips a locked attachment.
- Rows are staged under the run ID and published atomically after every batch
  validates. Publication prunes older runs.
- A failed run keeps the last published snapshot visible and marks the attachment `failed`.
- A crashed run's staged rows are deleted by the next lock holder.
- Detaching deletes runs and snapshots in both environments. Re-attaching creates a
  new attachment, so a run begun before the detach cannot publish.
- Deleting an environment's assignments (the nonproduction purge) deletes that
  environment's runs and snapshots, including runs in flight; the other
  environment is unaffected.
- A dataset whose definition is no longer deployed is skipped with a `WARN`
  (`dataset definition unavailable; refresh skipped`). Attaching an unknown dataset
  is rejected.

**Exit status.** Nonzero for a configuration error, an unreadable attachment list, a
failed dataset refresh (`failed > 0`), or a demographics failure. Skipped attachments
do not affect it. The run ends with an `INFO` summary of published, skipped, and
failed counts.

## Snapshots and presentation

Each run stores the `dataset.yaml` definition it ran under, and a snapshot is always
served with that definition. Changing or removing deployed dataset YAML cannot
reinterpret old results. The fingerprint and the "definition changed" notice cover
the dataset only; the UI shows the notice when the deployed fingerprint differs from
the snapshot's. Title and description come from the deployed definition, else the
latest published run, else the dataset name.

Reports are not captured. Each request binds the deployed reports of an attached
dataset to the snapshot's captured definition and summarizes the snapshot rows, so a
report edit appears after the next deploy without a refresh. A report that no longer
fits the captured definition returns `state: "incompatible"` with the title and the
dataset field at fault, and no summary data. The reason is `unknownField` (the
snapshot's definition lacks the field) or `incompatibleType` (its type no longer fits
the report). The report appears after the next successful refresh under the deployed
definition. Removing a dataset package requires removing its reports; the attachment
then keeps its list entry (`definitionAvailable: false`) and CSV download, which use
the captured definition, but shows no report card.

Owners attach and detach datasets in a study's **Datasets** tab. Members view
reports and download CSV. Attachments cover both environments; snapshots, reports,
and downloads are per environment. Dataset access follows study roles.

CSV columns are `randomization_id`, `arm_id`, `arm_name`, then the declared fields in
order. Cells are quoted and formula-like text is prefixed with an apostrophe. Context
values are never exported.

A report has one column per study arm in configuration order, including arms with no
rows. Arms present only in the snapshot follow, under their stored names. Percentages
use all snapshot rows of the arm; continuous statistics exclude nulls and report an
SD only from two values.

## Tests

`cargo test --workspace` covers contracts, conversions, CSV protection, and report
math. The ignored suites need a disposable PostgreSQL database:

```sh
export DATASET_TEST_DATABASE_URL=postgres://USER:PASSWORD@HOST:PORT/DATABASE
cargo test -p infra-pg -p api-http -p pocr-sync -- --ignored
```

The SQL Server driver test needs a disposable server with
`crates/infra-caboodle/tests/dataset_fixture.sql` installed. Set
`DATASET_TEST_SQLSERVER_HOST`, `_PORT`, `_DATABASE`, `_USERNAME`, and `_PASSWORD`, then run
`cargo test -p infra-caboodle --test datasets -- --ignored`.

The frontend demo is described in
[frontend/README.md](../../frontend/README.md).
