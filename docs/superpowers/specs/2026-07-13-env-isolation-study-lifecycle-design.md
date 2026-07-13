# Environment Isolation & Study Lifecycle — Design

**Date:** 2026-07-13
**Status:** Approved (ready for planning)

## Problem

1. **No environment isolation.** Epic TST and PRD instances hit the same CDS Hooks
   endpoint. A TST randomization request currently consumes the same permuted-block
   sequence and writes to the same `randomizations` table as PRD, corrupting
   allocation balance and the production audit trail.
2. **No lifecycle enforcement.** `StudyStatus` is `Draft | Active | Inactive` with no
   transition rules, no config lock, and no status gate in `Engine::randomize` — a
   draft study will randomize.

## Decisions (locked)

- **Environment signal:** the CDS Hooks `fhirServer` base URL. No Epic-side config change.
- **Mapping source:** static, startup-loaded config / env var → `Vec<(url_prefix, Environment)>` in `AppState`. Adding an instance requires a redeploy.
- **Unknown/missing URL:** reject (fail closed). No block consumed, no record written.
- **Isolation model:** env-scoped *state* (blocks, dedup, records), single shared *config*. Config frozen on Draft→Active go-live.
- **Lifecycle:** `Draft → Active → Paused → Closed`.
- **Env × status gating:** TST randomizes in any non-terminal state; PRD only when Active; Closed rejects both.

## Section 1 — The `Environment` concept

Add to `core`:

```rust
pub enum Environment { Production, NonProduction }
```

Resolution happens at the single randomize entry point (CDS Hooks handler). Match
`request.fhir_server` (`CdsHookRequest.fhir_server`, already present) against the
startup-parsed prefix map.

- Env var, e.g. `FHIR_ENV_MAP="https://prd.epic.ucsf.edu/...=production;https://tst...=nonproduction"`.
- Parsed once into `Vec<(String /*url_prefix*/, Environment)>`, held in `AppState`.
- **Unknown or missing `fhir_server` → reject** with an info card. Fail closed.

## Section 2 — Data partitioning by environment

`environment` partitions all *state*; the `Study` config row stays single/shared.

**Randomizations table** — add `environment` and repartition the uniqueness index:

```sql
ALTER TABLE randomizations ADD COLUMN environment TEXT NOT NULL DEFAULT 'production';
DROP INDEX idx_randomizations_study_unit;
CREATE UNIQUE INDEX idx_randomizations_study_env_unit
  ON randomizations(study_id, environment, unit_hash);
```

Same patient may be randomized once in TST and once in PRD, independently.
`RandomizationRepository::find_by_study` and `exists_for_unit` gain an
`environment` parameter. (`DEFAULT 'production'` backfills existing rows; may drop
the default post-migration for explicitness.)

**Block cache key:** `block:{env}:{study_id}:{stratum_key}` (currently
`block:{study_id}:{stratum_key}` in `RedisBlockCache::block_key`). TST and PRD draw
independent sequences; TST can never advance PRD's block or skew its balance.
`BlockCacheRepository` methods (`get_block`, `save_block`, `consume_next`) take an
`environment`.

**In-flight dedup key** (`Engine::randomize`, `dedup_key`): `{env}:{study_id}:{unit_hash}`.

**Purge (admin):** because TST state is namespaced —
`DELETE FROM randomizations WHERE study_id=$1 AND environment='nonproduction'`
plus a Redis scan-delete of `block:nonproduction:{study_id}:*` — cleanly resets test
data without touching PRD. Exposed via an admin endpoint.

## Section 3 — Study lifecycle state machine

```rust
pub enum StudyStatus { Draft, Active, Paused, Closed }
```

Transitions enforced in a single core method `Study::transition_to(next)`:

| From | To | Guard |
|------|-----|-------|
| Draft | Active | config validation passes; **freezes config** |
| Active | Paused | — |
| Paused | Active | — (config stays frozen) |
| Active | Closed | terminal |
| Paused | Closed | terminal |
| Draft | Closed | abandon a never-launched study |

Any other transition → `InvalidTransition { from, to }`. `Closed` is terminal.

**Config freeze:** once a study leaves `Draft`, the allocation config (arms, ratios,
keys, strata, block_size) is immutable. Editing frozen fields → `ConfigFrozen`.
`name`/`description` edits and locator assignment remain allowed (operational, not
allocation-affecting). Enforced in core.

**API split:** cosmetic edits stay on `PATCH /studies/:id`; status changes move to
`POST /studies/:id/transition` which runs the state machine. Current `update_study`
handler is refactored accordingly.

## Section 4 — Randomization gating (env × status)

At the top of `Engine::randomize`, after resolving environment:

| Study status | NonProduction | Production |
|--------------|:-:|:-:|
| Draft | ✅ | ❌ |
| Active | ✅ | ✅ |
| Paused | ✅ | ❌ |
| Closed | ❌ | ❌ |

**PRD randomizes only when Active. TST randomizes in any non-terminal state. Closed
rejects both.** Rejections return an info card ("Study not active" / "Study closed"),
not a 500. Implemented as a pure, table-driven decision function.

## Section 5 — Testing & error handling

**Core unit tests**
- `transition_to`: all allowed transitions + a representative set of forbidden ones.
- Config-freeze enforcement (mutating frozen fields rejected; name/description allowed).
- Env × status gating decision (pure function, table-driven, all 8 cells).
- Environment URL-prefix resolution incl. unknown/missing → reject.

**Service tests** (extend existing in-memory `Engine` tests)
- TST and PRD produce independent records for the same unit.
- Block keys differ by environment.

**New error variants**
`InvalidTransition { from, to }`, `ConfigFrozen`, `StudyNotActive`, `StudyClosed`,
`UnknownEnvironment`. Mapped to appropriate HTTP responses / CDS info cards in `api-http`.

## Scope / out of scope

- Migrations are additive and backfill-safe.
- **Out of scope:** frontend UI to drive transitions and view per-env data. API-first;
  UI follows once the API lands.
