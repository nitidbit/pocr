# Live Study Migration: POC Redis → POCR Postgres

**Date:** 2026-07-15
**Status:** Approved for implementation

## Problem

A live clinical-trial study is running on the legacy POC engine
(`~/Developer/UCSF/cloud-randomization/satellite/service`), which persists
**everything** in Redis as JSON strings. It must move to the POCR engine
(this repo), which uses **PostgreSQL as primary storage** with **Redis as a
permuted-block cache only**. No randomization data may be lost, and because
the study is live and uses **permuted block randomization**, in-flight block
state must survive the cutover intact or allocation balance is perturbed.

Brief downtime is acceptable, so this is a one-shot ETL — no dual-write.

## Approach

A dedicated, re-runnable Rust binary `pocr-migrate` (new workspace member at
`backend/apps/migrate`) that:

1. Reads the legacy Redis directly.
2. Transforms records in typed Rust against the **real** `pocr-core` models.
3. Writes new PostgreSQL rows and new Redis block state.

Linking `pocr-core`, `infra-pg`, and `infra-redis` means every field mapping
is compile-checked and reuses the exact `String::from(&unit)` /
`String::from(&strata)` functions the live engine uses — so `unit_hash` and
`strata_key` are guaranteed to match what the running engine would compute.

Rejected alternative: a `redis-cli` + `psql` script. It would hand-reimplement
the proto→serde transform and the unit/strata hashing in a second language with
zero schema guarantees — unacceptable risk for a live study.

## Source data (legacy Redis)

The legacy engine serializes **prost/tonic-generated proto types** to JSON
(integer enums, `oneof` shapes, snake_case fields, unix-epoch i64 timestamps).
Keys:

| Key pattern | Content |
|---|---|
| `study:{id}` | proto-JSON `Study` (id is a UUIDv4 string) |
| `randomization:{study_id}:{unit}` | full `Randomization` JSON, UUIDv4 `id` |
| `locator:{locator}` | `{"study_id": "..."}` |
| `block:{study_id}:{stratification}` | permuted block `{study_id, stratification, index, values}` |

## Target data (POCR)

- Postgres tables: `studies`, `randomizations`, `locator_mappings`,
  `user_studies` (see `crates/infra-pg/migrations`).
- Redis block cache: key `block:{study_id}:{stratum_key}`, value is a
  serialized `pocr_core::random::RandomPermutedBlock`.

The POCR `RandomPermutedBlock` has an `id: Uuid` field with **no
`#[serde(default)]`** — legacy block JSON lacks it, so it must be injected
during transform or deserialization into the new type fails.

## ID conversion: v4 → v7 (ALL IDs)

All UUIDs are converted from v4 to v7. **Each v7 is minted with its timestamp
component set to the record's original time** (not the migration instant), via
`uuid::Builder::from_unix_timestamp_millis(millis, &random_bytes)` with v7
version/variant — so the new IDs sort in true historical order, which is the
entire point of v7. Minting at `now_v7()` would cluster all records at the
migration instant and is explicitly rejected.

### Two-pass design

**Pass 1 — build old→new maps (in memory + persisted manifest):**

| Map | One entry per | v7 timestamp source |
|---|---|---|
| `study_id` | `study:*` key | study `created_at` |
| `arm_id` | each arm across all studies | owning study `created_at` |
| `randomization_id` | `randomization:*` key | record `issued_at` |

**Pass 2 — apply maps to every reference:**

- **new `study_id`** lands in: `studies.id`, `randomizations.study_id`,
  `locator_mappings.study_id`, `user_studies.study_id`, and the Redis block
  key `block:{new_study_id}:{stratum_key}`.
- **new `arm_id`** lands in: study config `arms[].id` **and** every
  `randomizations.arm` JSONB `id` — the same arm in two places, which must
  agree.
- **new `randomization_id`** lands in `randomizations.id` only (nothing
  references it).

### Block state is safe from arm remapping

Redis block `values` hold arm **indices**, not arm IDs. Arm remapping never
touches block contents. Blocks need only their **key**'s `study_id` rewritten;
`index` and `values` are copied byte-for-byte so a partially-consumed block
resumes mid-sequence.

### External-reference safety

The legacy CDS-Hooks external key is the **locator**
(`get_study_for_locator`), not the study id — so remapping study ids does not
break Epic integration. The persisted `migration-manifest.json` (all three
old→new maps) is the audit trail and reconciliation aid should any external
consumer have cached an old study id.

## Field transforms

### `study:*` → `studies`

| Field | Legacy (proto-JSON) | POCR | Transform |
|---|---|---|---|
| `id` | UUIDv4 string | UUIDv7 | remap via study map |
| `status` | i32 (1/2/3) | `"draft"`/`"active"`/`"inactive"` | int→string enum |
| `randomization_config.strategy` | i32 (1=Simple, 2=Block) | `"simple"`/`"block"` | int→string enum |
| `randomization_config.block_size` | u32 (0 = dynamic) | `Option<u32>` | `0 → None` |
| `randomization_config.arms[]` | `{id, ratio, locator}` | `{id, name, ratio, locator}` | remap `id`; `name = ""` |
| `randomization_config.keys[]` | proto `oneof {variant}` | `Option<KeyVariant>` (camelCase) | explicit variant map |
| `randomization_config.strata[]` | `{id,name,context,path}` | same | passthrough |
| `created_at` / `updated_at` | i64 unix seconds | `DateTime<Utc>` | epoch→timestamptz |
| `organization` / `created_by` | present | — | dropped; optionally seed a `user_studies` owner row from `created_by` if a real user id is present |

### `randomization:*` → `randomizations`

- `id` — remapped via randomization map (v7 from `issued_at`).
- `study_id` — remapped via study map.
- `unit_hash = String::from(&unit)` and `strata_key = String::from(&strata)`
  computed through the **shared core functions** (identical logic in both
  codebases → hashes match).
- `arm` JSONB — arm with remapped `id`.
- `issued_at` — passthrough (parsed epoch/RFC3339 → timestamptz).
- `epic_patient_id` / `csn` / `epic_provider_id` — `NULL` (legacy engine never
  captured them).

### `locator:*` → `locator_mappings`

`{locator (BIGINT) → new study_id}`. Trivial.

### `block:*` → new Redis `block:{new_study_id}:{stratum_key}`

Deserialize legacy JSON → inject `id` minted as v7 from... there is no
per-block timestamp in the legacy data, so block `id` is minted with
`Uuid::now_v7()` (blocks are transient cache state; ordering is irrelevant and
no record references a block id). Preserve `study_id` (remapped), `stratification`,
`index`, and `values` exactly. Reserialize as `pocr_core::random::RandomPermutedBlock`.

## Idempotency

All Postgres writes are upserts (`INSERT ... ON CONFLICT DO UPDATE`) keyed by
the new PKs; block writes are plain `SET`. Because Pass 1 mints v7 IDs from
stable source timestamps + random bytes, **re-running the bin produces new
random bits and therefore new IDs** — so idempotency requires the manifest:
on a re-run the bin loads an existing `migration-manifest.json` if present and
reuses its maps rather than re-minting. A fresh run (no manifest) is a clean
from-scratch migration.

## Runbook (brief-downtime cutover)

1. `BGSAVE` the legacy Redis; copy `dump.rdb` aside — rollback point.
2. Stop the legacy satellite service (**downtime begins**).
3. Run `pocr-migrate` with legacy-Redis connection (read) and POCR
   Postgres + Redis connections (write). Writes `migration-manifest.json`.
4. **Validate** (below).
5. Cut the CDS-Hooks / DNS endpoint over to the POCR engine.
6. Smoke-test one live randomization end-to-end.
7. Retain `dump.rdb` and the manifest until confident.

## Validation

- **Count parity:** studies, randomizations, locator_mappings, and blocks
  counts match source Redis.
- **Determinism check:** for N sampled units, issue a randomization request to
  the POCR engine and assert it returns the **same arm** already recorded
  (served from Postgres, must not re-randomize). This transitively validates
  study config + arm remapping + unit hashing.
- **Block integrity:** every migrated block's `index` and `values` are
  byte-identical to source (arm indices unchanged).
- **Manifest completeness:** every legacy id appears exactly once in the
  manifest with a valid v7 counterpart.

## Known post-cutover behavior (not a migration defect)

The POCR `find_by_study` reload reconstructs `unit` as a placeholder
`PatientId(unit_hash)` and drops `strata`
(`crates/infra-pg/src/randomization_repo.rs:105-125`). The debug/history view
therefore renders units differently than the legacy engine did. This is
pre-existing engine behavior, independent of migration, and is not data loss —
`unit_hash`/`strata_key` retain the full information the engine persists.

## Out of scope

- Dual-write / zero-downtime cutover.
- Backfill of `epic_patient_id` / `csn` / `epic_provider_id` (legacy never had
  them; the separate Caboodle sync pipeline populates them going forward).
- Migrating any study not currently live.
