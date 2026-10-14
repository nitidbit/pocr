# POCR Feature Status — Code-Verified

**Date:** 2026-07-24
**Method:** Each item from the inherited product/status doc was audited against the actual codebase (frontend, HTTP API, domain core, and integrations) at commit `49802e2`. "Previous status" is what the inherited doc claimed; "Verified status" is what the code shows.

> Numbering matches the source doc for easy cross-reference (there is no #9; the list jumps from 8 to 10).
>
> **See also:** [external-dev-guide.md](external-dev-guide.md) — per-item paths for developing each feature without Epic/UCSF-internal access (same numbering).

**Sprint history (added 2026-07-28, per the project manager):** Sprint 1 covered items 12, 13, and 14; Sprint 2 covered items 1 and 3. Those are five of the eight items flagged ⚠️ below — for them, the mismatch is best read as the inherited doc *predating the sprint work* (staleness), not as the doc being wrong when written. The remaining ⚠️ items (2, 11, 16) still have no in-repo explanation for their claims.

**Legend:** ✅ built · 🟢 substantially built (minor gaps) · 🟡 partially built · ❌ not started · ⚠️ inherited doc status did not match the code

---

## 1. Study management / admin portal (MVP) 🟢⚠️

**Description:** Centralized UI to configure studies, capture context, monitor enrollments, and manage reporting (alternative to REDCap).

**Previous status:** "Scoping needed; pinned for later. Requirements gathering planned."

**Sprint history:** Sprint 2 work — which explains the gap between "scoping needed" and what the code shows. The inherited status predates the sprint.

**Verified status: Substantially built — far beyond scoping.** Studies list, detail, create wizard, activation/lifecycle controls, and per-study monitoring are all wired to a real backend API (`/api/v1/studies` CRUD + transition + share/join), with a typed client (`frontend/src/api/client.ts`) and TanStack Query. No mock data in production paths.

Known gaps:
- The create wizard **silently discards** Team & Timeline and IRB/protocol fields (PI, coordinator, statistician, sites, dates, target enrollment, IRB number, protocol link) — collected and shown on the review step, but `CreateStudyRequest` has no fields for them (`frontend/src/routes/_app/studies/new.tsx`).
- The "Configuration" tab is read-only; randomization config cannot be edited after creation, and name/description editing isn't surfaced in the UI.
- Reports, Users, Context, and Strata pages are "Coming soon" placeholders.

## 2. Enhanced monitoring and analytics 🟡⚠️

**Description:** Real-time dashboards and alerts for randomization firing, parity checks, and study progress.

**Previous status:** "Recognized need. CSV export exists; admin portal needs being explored."

**Verified status: Monitoring page is real; CSV export claim is false.**
- Built: per-study monitor tab with arm counts/percentages, demographic balance with color-coded SMD badges, and recent randomizations — all from live endpoints (`frontend/src/routes/_app/studies/$studyId/monitor.tsx`). System health page (PG/Redis status + latency, TLS cert expiry) polling every 30s.
- **CSV export does not exist anywhere** — no export code in frontend or backend (repo-wide grep clean).
- No alerts of any kind; no true real-time (30s stale-time, no push/refetch on the monitor).

## 3. Caboodle integration (Epic EDW) 🟡⚠️

**Description:** Push randomization and context data to Caboodle with configurable tables for native site reporting.

**Previous status:** "Planned. Initial connectivity targeted in short term."

**Sprint history:** Sprint 2 work. Note the implication: what the sprint actually delivered is the *pull*-direction demographics sync described below, so either the team deliberately reinterpreted this item or the doc's push-to-Caboodle description is off — worth confirming with the manager which direction is the actual requirement.

**Verified status: Built in the OPPOSITE direction, and not deployed.** The `backend/crates/infra-caboodle` crate **pulls** demographics (PatientDim), race, insurance, and flowsheet BMI **from** Caboodle (MS-SQL via tiberius) and a `pocr-sync` binary upserts them into POCR's own `patient_demographics` table to power the balance report. **Nothing pushes randomization data to Caboodle.**

Production-readiness gaps: TLS cert validation disabled (`trust_cert()`), no retries/timeouts/pooling, zero tests, N+1 query for BMI, and `pocr-sync` is a one-shot binary that is **not in either compose file, not built into any Docker image, and has no scheduler** — it can only be run manually.

## 4. Default context capture + easy mapper 🟡

**Description:** Always capture a standardized set of FHIR/encounter fields by default; simple mapping tools for study-required context.

**Previous status:** "Priority agreed. Encounter ID→CSN mapping started; flowsheet support not built."

**Verified status: Default capture is implemented; the mapper UI is not.** Every randomization records `epic_patient_id`, `epic_csn` (extracted from prefetch by Epic OID suffix), and `epic_user_id` (`backend/crates/core/src/random/request.rs`; migration 0005). Study-specific context mapping exists as JSON-pointer FHIR paths on strata (`core/src/random/stratum.rs`).

Caveats: extraction is best-effort — missing identifiers become silent `NULL`s rather than errors; the "Context Configuration" and "Strata Management" UIs are placeholders; suspected bug: strata use capitalized context keys (`"Patient"`) while Epic prefetch keys are lowercase (`"patient"`) — needs verification. Flowsheet stratification: see #17.

## 5. Non-EHR randomization channels (contact center / Salesforce) ❌

**Previous status:** "Pilot planned (1-week Salesforce); configuration scaffolding needed."

**Verified status: Matches doc — zero code.** No Salesforce/contact-center code anywhere. Important prerequisite: there is **no direct REST randomize endpoint** — CDS Hooks (`POST /cds-services/randomization`) is the *only* randomization entry point, so any non-EHR channel requires new API surface first.

## 6. REDCap integration options ❌

**Previous status:** "Under active investigation; latency characterized."

**Verified status: Consistent — zero REDCap code, config, or docs in the repo.** Any investigation/latency work happened outside this codebase.

## 7. Adaptive randomization ❌

**Previous status:** "Conceptual; possible but not implemented."

**Verified status: Confirmed not implemented.** Strategies are `Simple` (weighted draw) and `Block` (permuted block via atomic Redis Lua consume); stratification is orthogonal config on either (`backend/crates/core/src/study.rs`, `random/block.rs`). No minimization, covariate- or response-adaptive code.

⚠️ Cleanup note: frontend types advertise `StratifiedBlock` and `Minimization` strategies and a `ClusterId` key that **do not exist in the backend** (`frontend/src/lib/types/randomization.ts`).

## 8. Blinded randomization workflows ❌

**Previous status:** "Gap identified; formal design pending."

**Verified status: Confirmed absent.** No blinding concept anywhere. Assignments are cleartext in the CDS card ("Randomized: {arm}") and in every read endpoint; stored roles are never used to mask visibility.

## 10. A/B BPA experimentation framework ❌

**Previous status:** "Not implemented."

**Verified status: Confirmed.** No variant tooling, no clickthrough/acknowledgment tracking. Card/suggestion UUIDs are modeled per CDS spec but there is **no `/cds-services/{id}/feedback` endpoint** and nothing consumes acceptance events.

## 11. Non-Epic EHR support via CDS Hooks ❌⚠️

**Previous status:** "Reflection service in progress; public sandbox testing underway."

**Verified status: Nothing in this repo.** Exactly one hook (`patient-view`), hardcoded to Epic: study locator read from `com.epic.cdshooks.request.criteria-id`, CSN by Epic OID, environment map keyed to UCSF Epic prod/non-prod URL prefixes. `fhirAuthorization` is modeled but never used — there is **no FHIR client at all** (prefetch-only; no live FHIR fetch capability). No Cerner/Athena/SMART-sandbox code. If a reflection service is in progress, it lives in another repo (possibly the satellite's).

## 12. Performance, scaling, and reliability 🟡⚠️

**Previous status:** "Preliminary server-side latency metrics collected; stress testing planned."

**Sprint history:** Sprint 1 work. The tracing infrastructure below is plausibly what the sprint delivered; the "latency metrics collected" claim likely refers to ad-hoc measurement outside the repo, since nothing in-repo collects metrics.

**Verified status: Overstated.** Structured tracing (tower-http request spans, JSON logs) exists, but there is **no metrics system** — no `/metrics` endpoint, no histograms, no Prometheus/OTel — and **no load tests or benchmarks** anywhere in the repo. The only latency numbers are point-in-time PG/Redis ping probes on `/system/status`.

Reliability note: the authoritative within-block allocation pointer lives in a **single Redis instance** (no Sentinel/Cluster/replication); losing Redis without a snapshot loses current block positions.

## 13. Security and identity management 🟡⚠️

**Previous status:** "Encryption added; signed/encrypted tokens implemented. Login/role-based access still needed."

**Sprint history:** Sprint 1 work — the Vouch/OIDC login below is presumably the sprint deliverable, and the doc's "login still needed" was written before or during that sprint. Authorization enforcement remains open regardless.

**Verified status: Login exists (doc stale); authorization is the top gap (worse than doc implies).**
- **Login:** implemented at the edge — Vouch OAuth/OIDC behind nginx; the engine trusts the `X-Vouch-User` header (`api-http/src/extractors.rs`). Debug builds auto-authenticate as `"system"`.
- **Tokens:** "signed/encrypted" is really HS256-**signed** study-share links only (7-day expiry, readable claims, `sharing.rs`). Not session auth, not encrypted.
- **Authorization: absent.** Roles (`owner`/`member`) are stored but **never enforced** — `get/update/delete/transition/purge/set_arm_locator/share`, `list_randomizations`, and `get_balance` do no membership checks. Any authenticated user who knows a study UUID can read or mutate it. An approved enforcement design (2026-07-16, in git history under the removed `docs/superpowers/specs/`) is unimplemented.
- **Edge gaps:** `/api/` and `/cds-services` are **not** behind nginx `auth_request`, and inbound `X-Vouch-User` is not explicitly stripped (header-spoofing surface). `/cds-services` has no mTLS/allowlist/shared-secret protection.
- **Deploy gaps:** `JWT_SECRET` and `FHIR_ENV_MAP` are not injected in `compose.yaml` — a release engine panics at startup without out-of-band injection. The satellite spec flags a committed Google OAuth client secret as still exposed.

## 14. Public "satellite" demo endpoint 🟡⚠️

**Previous status:** "Proposed; feasible; not yet hosted."

**Sprint history:** Sprint 1 work — consistent with the audit finding that a satellite was already running by 2026-07-10. "Not yet hosted" predates the sprint.

**Verified status: One was already running.** A 2026-07-10 design spec (git history) documents a live satellite stack (nginx + vouch + service + redis, images under `andrewdrobinson/ucsf`) on UCSF host `qccrndlap701`, maintained in a separate repo. Whether it is *publicly* reachable is unconfirmed, but "not yet hosted" was wrong when the doc was written.

## 15. Enhanced error handling for FHIR gaps 🟡

**Previous status:** "Identified as desired; not yet implemented."

**Verified status: Partially implemented, deliberately layered.**
- Fail-closed with clinician-visible CDS cards: unknown/missing `fhirServer` environment, missing Epic locator, and lifecycle-gate rejections all return HTTP 200 + informational cards (`api-http/src/cds_hooks.rs`).
- Missing *required* randomization keys raise typed `MissingField` errors — but currently surface to Epic as bare **500s**, not friendly cards.
- Missing *audit* identifiers (CSN, patient, user) go silently `NULL` — exactly the troubleshooting gap this item describes.
- No notifications/inbox infrastructure anywhere (no email, webhooks, queues, or alerting).

## 16. Study reporting UI and parity tools 🟡⚠️

**Previous status:** "Specs to be defined. CSV export exists."

**Verified status: Inverted — parity is built; CSV is not.** `GET /studies/{id}/balance` computes a table-1: age/BMI per-arm mean±SD, categorical counts/percentages (sex, ethnicity, language, smoking, insurance, multi-valued race), max pairwise SMD with a 0.1 imbalance threshold, and small-cell suppression (counts <5 redacted). Well unit-tested (`api-http/src/balance.rs`, `smd.rs`) and rendered on the monitor tab. **CSV export does not exist**; the Reports page is a placeholder.

## 17. Flowsheet row support for stratification (not MVP) ❌

**Previous status:** "Not built."

**Verified status: Confirmed.** Flowsheet data appears only in the offline Caboodle reporting pull (BMI, hardcoded Epic row id `14`) — never in the randomization path. Also related: `FhirPath` works as a *stratum* mapping but is `NotImplemented` as a *unit* key (`core/src/random/unit.rs`).

---

## Findings not in the original doc

- **gRPC is a dead path:** `crates/api-grpc` is an empty stub (placeholder function, no deps, no protos, not wired into the engine), yet `compose.yaml` exposes `:50051` and nginx terminates gRPC TLS on `:1443` into nothing.
- **Frontend/backend contract drift:** study statuses (`inactive` vs backend `paused`/`closed`), phantom randomization strategies (#7), and the wizard's dropped fields (#1) indicate the API contract and UI evolved separately.
- **Legacy migration tool:** `backend/apps/migrate` is a one-shot, idempotent Redis→Postgres ETL from the legacy engine (UUID v4→v7, manifest-tracked) with a good operator README — relevant if data ever needs re-migrating from the UCSF deployment.

## Suggested priority order

1. **Authorization enforcement** (#13) — highest-risk gap for a clinical-trial system; approved design already exists in git history.
2. **Edge hardening** (#13) — `auth_request` on `/api/`, strip inbound `X-Vouch-*`, protect `/cds-services`.
3. **Create-wizard data loss** (#1) — silently dropped IRB/team/timeline fields will surprise study teams.
4. **Deploy blockers** (#13) — `JWT_SECRET` / `FHIR_ENV_MAP` injection in `compose.yaml`.
5. **CSV export** (#2/#16) — small scope, closes the doc's most-repeated false claim.
