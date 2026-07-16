# pocr-migrate

One-shot ETL that migrates a live study from the legacy POC engine's Redis-only
store to the POCR PostgreSQL (primary) + Redis (block cache) stores.

It reads the legacy Redis directly, transforms records in typed Rust against the
real `pocr-core` models, and writes new Postgres rows and Redis block state.
Runs are **idempotent**: a persisted `migration-manifest.json` records the
old→new UUID maps so a re-run reuses the same IDs rather than re-minting them.

Full design, rationale, cutover runbook, and validation steps live in the spec:
[`docs/superpowers/specs/2026-07-15-live-study-redis-to-postgres-migration-design.md`](../../../docs/superpowers/specs/2026-07-15-live-study-redis-to-postgres-migration-design.md).

## What it does

- Migrates `study:*`, `randomization:*`, `locator:*`, and `block:*` keys.
- Converts all UUIDs from v4 → v7, minting each v7 from the record's **original**
  timestamp so historical ordering is preserved.
- Tags every migrated randomization `environment = production` and writes block
  state under `block:production:{study_id}:{stratum_key}`.
- Preserves in-flight permuted-block state (`index`/`values`) byte-for-byte so
  allocation balance is not perturbed at cutover.

## Configuration

Set via environment variables:

| Variable | Required | Default | Purpose |
|----------|----------|---------|---------|
| `LEGACY_REDIS_URL` | **yes** | — | Source: legacy POC Redis (read-only). |
| `POCR_REDIS_URL` | **yes** | — | Target: POCR Redis block cache (write). |
| `DATABASE_URL` | no | `postgres://pocr:pocr@localhost:5432/pocr` | Target: POCR Postgres (write). |
| `MIGRATION_MANIFEST` | no | `migration-manifest.json` | Path to the idempotency manifest. |

Standard observability vars (`RUST_LOG`, `LOG_JSON`, …) apply; the default log
filter is `info,sqlx=warn`.

## Running

```bash
LEGACY_REDIS_URL="redis://:<password>@legacy-host:6379" \
POCR_REDIS_URL="redis://pocr-redis:6379" \
DATABASE_URL="postgres://pocr:pocr@pocr-db:5432/pocr" \
cargo run -p pocr-migrate --release
```

On startup the binary **runs the POCR Postgres migrations itself**
(`infra_pg::run_migrations`) before migrating any data, so point it at a
reachable Postgres and it will bring the schema up to date. On success it logs
per-entity counts (studies, randomizations, locators, blocks) and the manifest
path, and exits `0`; any failure logs the error and exits non-zero.

The run is safe to repeat — re-running with the same manifest reuses the
existing ID maps and upserts, so a partial or retried migration converges.

## Cutover

This binary is one step in the cutover runbook. The full brief-downtime
sequence — `BGSAVE` the legacy Redis, stop the legacy service, run this tool,
validate (count parity, same-arm re-request, block integrity, manifest
completeness), then repoint the CDS-Hooks endpoint — is documented in the spec's
**Runbook** and **Validation** sections (linked above). Retain the legacy
`dump.rdb` and the manifest until the cutover is confirmed.
