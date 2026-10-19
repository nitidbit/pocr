# POCR External-Developer Guide — Working Without Epic Access

**Date:** 2026-07-28
**Context:** Companion to [feature-status.md](feature-status.md); item numbering matches. Premise: development is done by a non-UCSF developer who will likely have SSH/dev access to a UCSF server but will **never** have access to Epic, and possibly not to other UCSF-internal systems (Caboodle, REDCap, the clinical network). This doc maps, per feature, what can be built externally, what to stub or mock, and what genuinely requires a UCSF partner.

---

## The architectural fact that makes this workable

**Epic never receives calls from POCR.** There is no FHIR client anywhere in the codebase (feature-status #11) — POCR is prefetch-only. Epic's entire involvement is: it POSTs one JSON document to `POST /cds-services/randomization` and renders the JSON card that comes back. The Epic boundary is a single, well-specified HTTP request/response pair, defined by the public CDS Hooks spec plus a few Epic extensions that are visible in the code (`com.epic.cdshooks.request.criteria-id`, Epic OID identifier suffixes).

Everything downstream of that boundary — randomization core, Postgres, Redis, the `/api/v1` API, the frontend — runs locally today: `compose.dev.yaml` (app, engine, postgres, redis) for development, `compose.yaml` (adds nginx + vouch) for the full production shape. Nothing in the runtime stack is UCSF-licensed or UCSF-hosted.

The other inaccessible system, Caboodle, is similar in kind: `infra-caboodle` reads a handful of tables over MS-SQL. A local MS-SQL container with a mini-schema replaces it for development (see #3).

## Highest-leverage first investment: a "fake Epic" harness

The repo currently contains **zero example CDS Hooks request payloads** — the only inline JSON in the hooks path is the discovery prefetch template (`api-http/src/cds_hooks.rs:29`). Build a small Epic simulator early; nearly every item below leans on it.

- A script/CLI that POSTs realistic `patient-view` requests: `hook`, `hookInstance`, `fhirServer` (matching `FHIR_ENV_MAP` entries), `context` (patientId / userId / encounterId), the Epic `extension` block with `criteria-id`, and a prefetch bundle keyed `patient` / `encounter` / `user` (lowercase — that is what the discovery template declares, and per the CDS Hooks spec the EHR echoes the declared keys).
- Synthetic patients: [Synthea](https://github.com/synthetichealth/synthea)-generated FHIR bundles, or hand-rolled minimal Patient/Encounter resources carrying the Epic OID-suffixed identifiers the extractor looks for.
- Failure modes as first-class cases: missing locator, unknown `fhirServer`, missing required context keys, missing CSN/patient identifiers, malformed JSON. This matrix doubles as the test suite for #15.
- Reuse it for: local dev, CI integration tests, load generation (#12), and seeding demo data for #1/#2/#16.

**The single most valuable artifact to request from UCSF:** two or three real CDS Hooks request bodies captured from Epic non-prod, with PHI redacted. Everything else can be approximated from the spec; a real payload definitively settles prefetch shape, identifier OID formats, and the context-key casing question (#4).

## Public resources that require no UCSF affiliation

- **CDS Hooks spec and sandbox** — cds-hooks.org, sandbox.cds-hooks.org
- **Epic on FHIR** (fhir.epic.com / open.epic) — free developer account; Epic's public CDS Hooks documentation (including request JWT signing) and public FHIR sandbox. Being non-UCSF does not lock you out of this.
- **SMART Health IT launcher** and public HAPI FHIR test servers
- **Synthea** — synthetic FHIR patient generation
- **REDCap API documentation** — public via Vanderbilt and partner-site mirrors
- **Salesforce Developer Edition** — free personal org for prototyping (#5)

Not publicly available: **Epic UserWeb / Galaxy** documentation, including the Caboodle data model. That requires an Epic-community login, which UCSF staff have — schema questions route through them.

## Trying the CDS Hooks sandbox locally

The public [CDS Hooks sandbox](https://sandbox.cds-hooks.org) can call a POCR engine running on your laptop, because the sandbox runs in your browser and the browser makes the calls. Chrome and Firefox allow an HTTPS page to call `http://localhost`; Safari does not. `compose.dev.yaml` already sets the sandbox settings (`CDS_CORS_ORIGINS`, `CDS_DISCOVERY_LIST_STUDIES`, and `launch.smarthealthit.org` in `FHIR_ENV_MAP`).

1. `docker compose -f compose.dev.yaml up`
2. Open `http://localhost:5179` and create a study (draft is fine), e.g. two arms, block randomization, key "patient ID".
3. In Chrome, open `https://sandbox.cds-hooks.org/?serviceDiscoveryURL=http://localhost:3000/cds-services`.
4. On the Patient View tab you should see a "Randomized: <arm>" card. Reloading shows the same assignment; _Change Patient_ gives a new one. Ignore the "No locator found" card from the Epic-style `randomization` service, or turn it off under gear → _Configure CDS Services_.

Never expose `compose.dev.yaml` beyond your own machine: its debug engine treats unauthenticated requests as a built-in user ([security-concerns.md](security-concerns.md) S5). For the sandbox against an installed server, see [install.md](install.md#try-it-with-the-public-cds-hooks-sandbox).

## The standing ask-list for UCSF

| Ask | Unblocks |
|---|---|
| 2–3 sanitized real CDS Hooks request payloads from non-prod | harness fidelity, #4 casing bug, #15 |
| Caboodle DDL for the tables/columns `infra-caboodle` touches | #3 |
| Confirmation: is the Caboodle requirement pull, push, or both? | #3 |
| `criteria-id` values and BPA build configuration in use | #10, #11 |
| Epic version + whether it sends CDS Hooks feedback | #10 |
| REDCap: API token on a throwaway dev project, or a partner to run integration tests | #6 |
| Satellite repo access | #14 |
| Epic's request-signing JWT `iss`/`jku` values for non-prod | #13 |
| Screenshots of POCR cards rendering in Hyperspace | #8, #15 |
| Rotate the committed Google OAuth client secret | #13 (UCSF must do this regardless) |

---

## Effort estimates: how to read the points

Each item below carries an **Effort** line using Fibonacci story points (1, 2, 3, 5, 8), sized on complexity and uncertainty — deliberately not mapped to time.

- **1** — trivial; a config or one-file change.
- **2** — small, well-understood; one focused session of work.
- **3** — modest; crosses the stack (e.g. API field + migration + frontend) but the path is known.
- **5** — a real feature with design decisions; the largest story worth taking into a sprint whole.
- **8** — complex or high-uncertainty; expect discovery, iteration, or statistical/security rigor. Split before scheduling if possible.

Anything that would have been 13+ is already split into stories, so items show a story list and a summed total (sums aren't forced to Fibonacci — only stories are). Three stories appear under multiple feature items; each is pointed once at a canonical home and referenced elsewhere: **CSV export** lives in #16, **notification/alerting infrastructure** in #2, **Context/Strata management UI** in #4. Estimates cover only the externally developable scope this doc describes — UCSF-side validation, Epic build work, and anything in the final "cannot be done" section are not pointed.

## Per-item development paths

### 0. Fake-Epic harness (cross-cutting prerequisite)

**Effort: 5** — the payload builder itself is a 3, but the failure-mode matrix, synthetic-patient generation, and CI wiring push it to a 5. Do it first: it de-risks nearly every 8-pointer below.

### 1. Study management / admin portal — fully local, no Epic dependency

Everything open here — the wizard's silently dropped fields, post-creation config editing, the placeholder Reports/Users/Context/Strata pages — is frontend + `/api/v1` + Postgres work. Populate the monitor tab with data by running the fake-Epic harness or a seed script. Blocked on UCSF: nothing.

**Effort: 13** — wizard field loss end-to-end (**3**: new `CreateStudyRequest` fields + migration + wiring, mechanical but crosses the stack) · post-creation config editing (**5**: the work is deciding what stays mutable after activation/first randomization, not the CRUD) · Users/sharing management page (**5**: only meaningful once #13's authorization lands). Context/Strata pages are pointed under #4; Reports under #16.

### 2. Enhanced monitoring and analytics — fully local

CSV export, alerting logic, and true real-time (SSE/WebSocket or aggressive refetch) are all local. Demographic balance needs `patient_demographics` rows — seed them synthetically, or via the mini-Caboodle sync from #3. Alert *delivery* endpoints (who gets paged/emailed) are a UCSF decision eventually, but the mechanism is buildable now against a local SMTP catcher (e.g. Mailpit).

**Effort: 11** — real-time monitor updates (**3**) · alerting (**8**: rule definitions, evaluation, and the first delivery channel — this is the canonical home for notification infrastructure, which #15 reuses). CSV export is pointed under #16.

### 3. Caboodle integration — mock with a local MS-SQL container

- Run MS-SQL locally: `mcr.microsoft.com/mssql/server:2022-latest` (works on Apple Silicon under Docker Desktop's emulation) or `azure-sql-edge` (ARM-native). Derive a mini-Caboodle schema from the exact queries in `infra-caboodle` — those queries *are* the contract — seed synthetic rows, and run `pocr-sync` end-to-end.
- Every production-readiness gap is testable against the fake: enabling TLS cert validation, retries/timeouts/pooling, fixing the N+1 BMI query, unit/integration tests, and wiring `pocr-sync` into compose with a scheduler.
- If the requirement turns out to be **push** (the open direction question flagged in feature-status), that is designable and implementable against the same container.
- UCSF-only: real DDL confirmation, and one supervised validation run on their network at the end.

**Effort: 10 (+8 conditional)** — mini-Caboodle container + seed data (**3**) · client hardening: TLS validation, retries/timeouts/pooling, N+1 BMI fix, tests (**5**) · scheduling `pocr-sync` in compose (**2**). If the answer to the direction question is *push*: **+8** — designing writes into a warehouse you cannot see is high-uncertainty until the DDL arrives; do not point it lower on optimism.

### 4. Default context capture + mapper — local; one question wants a real payload

Extraction is pure code over prefetch JSON — fully unit/integration-testable with synthetic bundles. The mapper UI ("Context Configuration", "Strata Management") is frontend work. The suspected casing bug is now *nearly* confirmed in-repo: the discovery template declares lowercase `patient`/`encounter`/`user`, and the spec says the EHR echoes declared keys, so strata configured with `"Patient"` would never match — one sanitized real payload settles it beyond doubt. Turning silent `NULL` audit identifiers into surfaced warnings: local.

**Effort: 10** — casing bug fix + regression tests (**2**) · Context/Strata management UI (**8**: a FHIR-path builder with validation against sample bundles is the hardest frontend work on this list; canonical home for the story #1 references). NULL-identifier warnings are pointed under #15.

### 5. Non-EHR channels (Salesforce / contact center) — mostly local

The prerequisite — a direct REST randomize endpoint (none exists today; CDS Hooks is the only entry point) — is pure backend work. Salesforce integration can be prototyped end-to-end against a free personal Developer Edition org; only the final connection to UCSF's org needs their admin. The contact-center flow needs UCSF *process* knowledge, not system access.

**Effort: 10 (+ unestimated)** — REST randomize endpoint (**5**: the route is easy; auth, idempotency, and audit parity with the CDS path are the real scope) · Salesforce prototype in a dev org (**5**). The contact-center flow is unestimable until UCSF describes the process.

### 6. REDCap — mock the API; integration needs a UCSF token or partner

REDCap cannot be self-hosted without an institutional license from Vanderbilt, but its API is small, public, and stable (token auth, POST-based record import/export). Path: define a REDCap adapter interface, build a mock server implementing the endpoints POCR would use, and develop the export against the mock. Then ask UCSF for either an API token on a throwaway project in their dev REDCap instance (usable externally if their network allows) or a UCSF partner to run the integration tests. The design questions — data direction, event/instrument mapping — are conversations, not access.

**Effort: 8** — adapter interface + mock REDCap server (**5**) · export implementation against the mock (**3**). Re-point after the design conversation: event/instrument mapping for longitudinal projects could grow this substantially.

### 7. Adaptive randomization — ideal external work; zero dependencies

Pure algorithm work in the `core` crate (minimization, covariate-adaptive strategies), validated with property tests and simulation. Also includes deleting the phantom frontend strategy types (`StratifiedBlock`, `Minimization`, `ClusterId`) or implementing them for real. Nothing external at all.

**Effort: 9** — phantom-type cleanup (**1**) · minimization (Pocock–Simon) with property tests and simulation (**8**: the algorithm itself is small; proving its statistical behavior, and wiring config → API → frontend, is the work).

### 8. Blinded randomization workflows — local build; UCSF for UX validation only

Role-based masking of assignments in API responses and in card text is backend + frontend work, testable with the harness (assert the card no longer contains the arm). The only UCSF dependency is cosmetic: a screenshot of how the blinded card renders in Hyperspace.

**Effort: 8** — one story, broad rather than deep: blinding touches every read path (API responses, CDS cards, monitor UI, exports) and needs an unblinding/audit policy designed before the first line of code. Depends on #13's authorization to have roles worth masking by.

### 10. A/B BPA experimentation — POCR half local; Epic half needs UCSF

The POCR side — a `/cds-services/{id}/feedback` endpoint per the public CDS Hooks spec, variant assignment, acceptance analytics — is fully local, with the harness simulating feedback POSTs. The Epic side — whether UCSF's Epic version actually sends feedback, and how BPA variants get configured — is Epic build territory. Get UCSF confirmation *before* investing heavily, since the analytics are worthless if Epic never calls the endpoint.

**Effort: 8** — feedback endpoint + storage (**3**) · variant assignment framework + acceptance analytics (**5**). Sequence the 3 first and gate the 5 on UCSF's confirmation that Epic will actually send feedback.

### 11. Non-Epic EHR support — surprisingly accessible externally

De-Epic-ing the hook handler (config-driven locator instead of `criteria-id`, generic identifier-system handling, generic environment map) is local refactoring, verifiable with the harness in "non-Epic mode". Public sandboxes to validate against: the CDS Hooks sandbox, SMART launcher, Oracle Health (Cerner) code console (free account), and Epic's own public sandbox at fhir.epic.com — which, notably, a non-UCSF developer *can* use. Adding a real FHIR client that honors `fhirAuthorization` is testable against those same sandboxes. Ask UCSF what/where the "reflection service" from the inherited doc lives (satellite repo?).

**Effort: 13** — de-Epic refactor (**5**: config-driven locator + generic identifiers, with harness-verified behavior preserved for the Epic path) · FHIR client honoring `fhirAuthorization` (**8**: OAuth2 client-credentials, token caching, fetch-on-missing-prefetch — an entirely new capability class for this codebase).

### 12. Performance / scaling / reliability — build locally, calibrate at UCSF

A `/metrics` endpoint (Prometheus/OTel), load tests (k6, goose) firing harness payloads at the local stack, and Redis resilience work (Sentinel in compose, or a snapshot/restore drill for the block-pointer loss scenario) are all local. Local numbers won't match UCSF hardware, but regressions and scaling *shape* are meaningful; one calibration run on the UCSF dev server (which the developer expects to get) closes the loop.

**Effort: 11** — `/metrics` endpoint with request/randomization histograms (**3**) · load-test suite driven by harness payloads (**3**) · Redis resilience (**5**: Sentinel-or-snapshot is a design decision with failover behavior to prove, not a config toggle — the block-pointer is authoritative state).

### 13. Security and identity — almost all local; two UCSF items

Local: authorization enforcement (the approved design in git history), nginx `auth_request` on `/api/`, stripping inbound `X-Vouch-*`, `JWT_SECRET`/`FHIR_ENV_MAP` injection in `compose.yaml`, and — importantly — CDS Hooks endpoint authentication: Epic signs its CDS Hooks requests with a JWT (publicly documented on open.epic), so verification can be implemented and tested locally against a self-hosted JWKS, with Epic's real `iss`/`jku` values dropped in later. Vouch login works locally with any Google account (or debug auto-auth). UCSF-only: rotating the committed OAuth secret, production IdP configuration, and the real Epic JWT parameters.

**Effort: 11** — authorization enforcement (**5**: the design is approved, but it touches every study endpoint and needs a test matrix per role × operation) · edge hardening: `auth_request` + header stripping (**2**) · CDS Hooks JWT verification against a test JWKS (**3**) · compose secret injection (**1**). Highest priority per feature-status; also unblocks #1's Users page and #8.

### 14. Public satellite demo — get repo access, or rehost independently

The satellite is a separate repo running on a UCSF host. Two paths: request access to that repo (it is a demo stack, presumably not PHI-bearing), or stand up an equivalent public demo on non-UCSF cloud infrastructure from `compose.yaml` + synthetic data. The second is arguably *better* for an external developer — full control, no UCSF network dependency, and it doubles as a persistent integration-test target. No Epic involved either way.

**Effort: 3** — rehost from `compose.yaml` on a cloud VM with TLS, a domain, and synthetic seed data. If satellite-repo access is granted instead, re-point after seeing it.

### 15. Enhanced error handling — local; the harness is the test rig

Converting `MissingField` bare 500s into friendly cards, surfacing warnings when audit identifiers come back `NULL`, and any notification infrastructure (local SMTP catcher) are all driven by the harness's failure-mode matrix. The only UCSF dependency is visual confirmation of how new cards render in Hyperspace.

**Effort: 4** — `MissingField` errors → friendly cards (**2**) · NULL-audit-identifier warnings (**2**). Delivery of those warnings reuses #2's notification infrastructure, pointed there.

### 16. Reporting UI and parity tools — fully local

CSV export and the Reports page sit on top of the existing, well-tested balance endpoints; seed data synthetically. One conversation (not access) for UCSF: confirm the small-cell suppression policy (currently counts < 5 redacted) matches their reporting requirements.

**Effort: 6** — CSV export, canonical home for the story #2 references (**3**: endpoint + frontend download, plus deciding what suppression rules apply to exports) · Reports page v1 over existing balance/randomization endpoints (**3**).

### 17. Flowsheet stratification — buildable on synthetic Observations

Flowsheet rows surface through Epic's FHIR layer as Observation resources, so the buildable core is: stratification over prefetched Observations, plus implementing `FhirPath` as a *unit* key (currently `NotImplemented` in `core/src/random/unit.rs`) — all testable with synthetic bundles. UCSF-only: real flowsheet row IDs, confirmation that prefetch can deliver the needed Observations, and the Epic build config. Low priority per the original doc (explicitly not MVP).

**Effort: 5** — `FhirPath` unit key + Observation-based stratification on synthetic bundles, one story. The Epic-side unknowns are not pointed; they could invalidate assumptions, so treat this as a spike-then-build.

---

## Effort rollup

| # | Item | Points | Notes |
|---|---|---|---|
| 0 | Fake-Epic harness | 5 | do first |
| 1 | Admin portal | 13 | Users page gated on #13 |
| 2 | Monitoring/analytics | 11 | home of notification infra |
| 3 | Caboodle | 10 (+8) | +8 only if push direction confirmed |
| 4 | Context capture/mapper | 10 | home of Strata/Context UI |
| 5 | Non-EHR channels | 10 (+?) | contact center unestimated |
| 6 | REDCap | 8 | re-point after design conversation |
| 7 | Adaptive randomization | 9 | zero external dependencies |
| 8 | Blinded workflows | 8 | gated on #13 |
| 10 | A/B BPA framework | 8 | 5 of it gated on UCSF confirmation |
| 11 | Non-Epic EHR support | 13 | FHIR client is the 8 |
| 12 | Perf/reliability | 11 | |
| 13 | Security/identity | 11 | highest priority; unblocks #1, #8 |
| 14 | Satellite demo | 3 | |
| 15 | Error handling | 4 | reuses #2 infra |
| 16 | Reporting/parity | 6 | home of CSV export |
| 17 | Flowsheet stratification | 5 | spike first |
| | **Total** | **145 (+8 conditional)** | shared stories counted once |

Reading the total: at a typical solo-developer cadence this is multiple quarters of work, which is the point of having it in one table — the sprint-planning conversation with UCSF should be about *which* rows, not whether all of them fit.

---

## What genuinely cannot be done without UCSF

Kept short, because it is short: Epic build/configuration changes and anything rendered in Hyperspace; end-to-end validation in Epic non-prod; live connectivity to real Caboodle or REDCap; anything touching PHI; rotating the leaked OAuth secret; production deploys and UCSF network/firewall changes. Everything else in this codebase has a credible external development path.
