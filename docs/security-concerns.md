# POCR Security Concerns — Running List

**Started:** 2026-10-14
**Purpose:** Track the security issues known in the current code, so documentation can describe them honestly and the fixes can be prioritized. Feature work is moving ahead in parallel; nothing here is a blocker for writing install docs, but several items belong in a site's security review packet.

Each entry: what it is, where it lives, who can exploit it, and what a site can do today. Mark items **Resolved** (with the commit) rather than deleting them.

> Supersedes the security notes in [feature-status.md](feature-status.md) #13 where they differ — two of those were stale as of this date (see "Corrected" at the bottom).

---

## Open

### S1. Study authorization is not enforced — **High**

Roles (`owner` / `member`) are stored, and the membership endpoints (`api-http/src/members.rs`) enforce them, but every other study endpoint does not: `get_study`, `update_study`, `transition_study`, `delete_study`, `purge_nonproduction`, `set_arm_locator`, `update_ratios`, `share_study`, `list_audit`, `list_randomizations`, `count_randomizations`, `get_balance`. Any user who can log in (anyone allowed by the Vouch config) and knows a study UUID can read or change it — including activating, closing, deleting, or re-pointing arm locators on a live trial.

- **Mitigation today:** admit only the people who run trials. Preferably assign the POCR application to a group in the identity provider (with `POCR_ALLOWED_EMAIL_DOMAINS` in the install's `.env`, or `domains` in `vouch/config.yml` for the repository's own `compose.yaml`). Where the identity provider can't do that, list named users (`POCR_ALLOWED_USERS`; `whiteList` in `vouch/config.yml`). Don't admit everyone in an email domain without one of these.
- **Fix:** an approved enforcement design (2026-07-16) exists in git history under the removed `docs/superpowers/specs/`.

### S2. CDS Hooks endpoints are unauthenticated — **High**

`/cds-services/*` is proxied by nginx with no `auth_request`, and the engine does not verify the JWT that Epic signs each CDS Hooks request with (the `Authorization: Bearer` header). Anyone who can reach the server over the network can:

- POST to `/cds-services/randomization` with a guessed locator (small integers — Epic OPA criteria IDs), or
- POST to `/cds-services/{study_id}` for any open study (added 2026-10-14 for the CDS Hooks sandbox; see S3),

and create real randomization records, consuming block slots and skewing allocation.

The only gate is `FHIR_ENV_MAP`: the request's `fhirServer` must match a configured prefix. But `fhirServer` is a field in the caller's own JSON body, so it is not a security control — it prevents *accidents* (a test instance writing into production), not attacks.

- **Mitigation today:** firewall the server so `/cds-services` is reachable only from the Epic Interconnect servers' addresses (`CDS_ALLOW_FROM` in the install's `.env`, which nginx enforces on that location, or a network ACL).
- **Fix:** verify Epic's CDS Hooks JWT (signature via the `jku` JWKS, plus `iss`, `aud`, `exp`). Epic's scheme is public on fhir.epic.com, so this can be built and tested locally against a self-hosted JWKS.

### S3. Service discovery can list every open study — **Medium, off by default**

With `CDS_DISCOVERY_LIST_STUDIES=true`, `GET /cds-services` (unauthenticated) returns one entry per non-closed study, including its UUID, name, and status. That is what makes the public CDS Hooks sandbox usable, but it also reveals study names to anyone who can reach the server, and hands out the UUIDs that S2's per-study endpoint accepts.

- **Default:** off in `compose.yaml`; on in `compose.dev.yaml`. Turn it on only for a demo server holding synthetic data. With it off, the per-study endpoint still works for anyone who knows the UUID (S2 still applies).

### S4. CORS on `/cds-services` — **Low, off by default**

`CDS_CORS_ORIGINS` (comma-separated origins, or `*`) lets browser-based CDS clients call the hook routes; the sandbox needs `https://sandbox.cds-hooks.org`. Unset means no CORS headers at all, which is correct for Epic (it calls server-to-server). When set, the Private Network Access header is also sent so a public page can reach `localhost` or an internal address.

CORS restricts which *web pages* can read responses in a browser; it does nothing against direct callers (that is S2). The risk it guards against: if a site protects POCR only by network location, an over-broad allow-list lets any web page a clinician visits reach the internal server through their browser.

- **Default:** unset in `compose.yaml`; `https://sandbox.cds-hooks.org` in `compose.dev.yaml`. Never use `*` on a server holding real data (the engine logs a warning if you do).

### S5. Debug builds authenticate everyone as `system` — **Low (deployment hygiene)**

`api-http/src/extractors.rs`: when `X-Vouch-User` is absent, debug builds fall back to the user `"system"`. Release builds return 401. The production Dockerfile target builds release, so this only bites if someone deploys the dev image or `compose.dev.yaml` on a reachable host.

- **Docs:** the install guide must say never to expose `compose.dev.yaml` beyond localhost.

### S6. Assignments are visible in clear text — **Depends on trial design**

No blinding: the CDS card states the arm, and every read endpoint returns it. Fine for pragmatic open-label trials; a problem for any trial that needs allocation concealment. See feature-status #8.

### S7. PHI is stored — **Informational, for the site's privacy review**

Each randomization row stores the EHR patient ID, encounter CSN, and user ID (`randomizations.epic_patient_id`, `epic_csn`, `epic_user_id`), and Redis holds a hash of the randomization unit. The optional Caboodle sync adds demographics (birth date, sex, race, ethnicity, language, insurance, BMI) to `patient_demographics`. The database volume and its backups therefore need the site's PHI handling.

---

## Corrected (feature-status.md #13 was stale)

- **"`/api/` is not behind nginx `auth_request`"** — no longer true. `nginx/templates/app.conf.template` runs `auth_request /validate` on `/api/` and `/api/me`.
- **"Inbound `X-Vouch-User` is not stripped"** — effectively resolved for `/api/`: `vouch-proxy-headers.conf` sets `X-Vouch-User` from the Vouch response, overwriting any client-supplied value. The engine port (3000) is not published in `compose.yaml`, so nginx is the only way in.
- **"Committed Google OAuth client secret"** — not in this repository: `vouch/config.yml` has an empty `client_secret` and no history of a real one. The leaked secret, if still live, is in the older satellite repo; UCSF should still rotate it.
