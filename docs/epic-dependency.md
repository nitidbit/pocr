# POCR and Epic — What Is Epic-Specific and How to Remove It

_Last updated: 2026-10-19_

POCR talks to the EHR over CDS Hooks, an HL7 standard. The randomization core (studies, arms, blocks, strata, the production/nonproduction split) uses only standard CDS Hooks and FHIR fields. The Epic dependence is at the edges: how a request is matched to a study, which identifiers are recorded, where balance demographics come from, and how requests will be authenticated.

This page lists each dependency, what happens on a non-Epic EHR today, and the work that would remove it. Paths are relative to `backend/crates/` unless they start with `docs/` or `deploy/`.

---

## Already EHR-neutral

- **Per-study endpoint.** `POST /cds-services/{study_id}` (`api-http/src/cds_hooks.rs`) reads no vendor extension. The study is in the URL and the card always names the assigned arm. The route is always served; `CDS_DISCOVERY_LIST_STUDIES` controls only whether discovery (`GET /cds-services`) lists it. The public CDS Hooks sandbox randomizes through this endpoint.
- **Environment map.** `FHIR_ENV_MAP` matches on any `fhirServer` URL prefix (`core/src/environment.rs`).
- **Strata and FHIR-path unit keys.** These read fields from whatever FHIR resources the EHR prefetches (`core/src/random/stratum.rs`, `core/src/random/key.rs`).
- **No outbound calls.** POCR never calls the EHR's FHIR API. Everything it needs arrives in the request's `prefetch`.

---

## Epic-specific today

### 1. Finding the study and arm (locator)

**Where:** `extract_locator` and `randomize` in `api-http/src/cds_hooks.rs`.

The main endpoint, `POST /cds-services/randomization`, identifies the study from the Epic extension field `com.epic.cdshooks.request.criteria-id`. That is the ID of the Epic OPA criteria record that fired, which POCR stores as an arm's *locator*. The flow assumes Epic's build pattern: one criteria record (and BestPractice Advisory) per arm, each calling POCR. A card is returned only when the assigned arm's locator matches the one that called.

**On a non-Epic EHR:** the extension is absent, so this endpoint returns a "No locator found" card. The EHR has to call the per-study endpoint instead, which has no per-arm gate.

### 2. Recorded identifiers

**Where:** `extract_epic_ids` in `core/src/random/request.rs`; columns `epic_patient_id`, `epic_csn`, `epic_user_id` on `randomizations` (`infra-pg/migrations/20250101000005_add_epic_ids_to_randomizations.sql`, `infra-pg/src/randomization_repo.rs`).

Apart from these columns, a randomization record stores the subject only as a hash (`unit_hash`). The three readable identifiers are pulled from the prefetched resources using Epic conventions:

| Column            | Source                                                                       |
| ----------------- | ---------------------------------------------------------------------------- |
| `epic_patient_id` | Patient identifier whose `type.text` is `EXTERNAL`                           |
| `epic_csn`        | Encounter identifier whose `system` ends in `.7.3.698084.8` (Epic CSN OID)   |
| `epic_user_id`    | Practitioner (`user`) identifier whose `type.text` is `EXTERNAL`             |

Extraction is best-effort: a missing match stores `NULL` with no error or log.

**On a non-Epic EHR:** randomization succeeds, but all three columns are `NULL`. Assignment records cannot be tied back to a patient, encounter, or clinician, and the balance report (which joins on `epic_patient_id`) is empty.

### 3. Balance demographics (Caboodle sync)

**Where:** `infra-caboodle/` and `apps/sync/`.

Demographics for the balance report come from an offline sync that queries Epic's Caboodle data warehouse over MS-SQL, including a hard-coded Epic flowsheet row for BMI. The storage side (`DemographicsRepository` in `core/src/ports.rs`, `infra-pg/src/demographics.rs`) is EHR-neutral.

**On a non-Epic EHR:** no demographics source exists, so the balance report has no data. (Many Epic sites will not run the sync either; it is optional.)

### 4. Request authentication (not yet built)

**Where:** [security-concerns.md](security-concerns.md) S2.

`/cds-services` does not yet verify the JWT the EHR signs each request with; `CDS_ALLOW_FROM` restricts it by address instead. The planned fix is described in Epic terms. The scheme it would check (`iss`, `aud`, `exp`, keys from the `jku` JWKS) is defined by the CDS Hooks specification, not by Epic.

**On a non-Epic EHR:** same as Epic today: address allow-list only.

### 5. Smaller items

- **Hook.** Only `patient-view` is advertised in discovery (`discover_services` in `api-http/src/cds_hooks.rs`). Other EHRs may support or prefer other hooks (for example `order-select` or `order-sign`).
- **Card link.** The assignment card's source URL is hard-coded to `https://randomization.ucsf.edu` (`assignment_card` in `api-http/src/cds_hooks.rs`).
- **Docs.** The install and overview docs assume Epic Interconnect and an Epic CDS/OPA analyst.

---

## Removing the dependency

In suggested order. Items 2 and 3 also fix problems for Epic sites (silent `NULL` identifiers, unauthenticated requests), so they are worth doing regardless.

1. **Make the per-study endpoint a supported integration, not a sandbox feature.**
   - Separate "this study's endpoint can be called" from "discovery lists every open study". Listing exposes study names and UUIDs to anyone who can reach discovery ([security-concerns.md](security-concerns.md) S3), so a site should be able to register one study's service URL without turning listing on.
   - For per-arm triggering without a vendor extension, add an arm-level service (for example `POST /cds-services/{study_id}/{arm_id}`) with the same gate the Epic flow applies.
   - Keep `POST /cds-services/randomization` as the Epic option.

2. **Make identifier extraction configurable.**
   - Rename the `epic_*` columns to generic patient, encounter, and user identifier columns (migration, repository, API response, balance join).
   - Replace the hard-coded rules with a per-install identifier profile: for each of patient, encounter, and user, match an identifier by `system` (exact or suffix) or `type.text`. Ship the current rules as the Epic profile and default.
   - Fall back to the FHIR resource IDs from the CDS Hooks `context` when no identifier matches.
   - Log a warning when a configured identifier is missing.

3. **Build S2 as generic CDS Hooks JWT verification.** Configure a list of trusted issuers, each with its JWKS URL (or allowed `jku` prefix) and expected audience. Epic is one entry. Keep `CDS_ALLOW_FROM` as defense in depth.

4. **Give balance demographics a non-Epic source.** Either:
   - **Capture at randomization time:** store the balance fields (birth date, sex, and so on) from the prefetched Patient resource. Works on any EHR with no extra system, but puts more PHI in POCR's database.
   - **Add another sync source** behind `DemographicsRepository`, such as a FHIR bulk export or a site's own warehouse.

   Caboodle then becomes one optional source.

5. **Small items.**
   - Make the advertised hook configurable per study.
   - Make the card's source URL a setting.
   - Split the EHR-integration docs into a generic CDS Hooks guide and an Epic-specific section.

6. **Test against a second CDS Hooks client.** The public CDS Hooks sandbox covers the protocol. For any specific EHR, first confirm which hooks and prefetch templates it supports, and get a de-identified request body captured from it, as [external-dev-guide.md](external-dev-guide.md) recommends for Epic.
