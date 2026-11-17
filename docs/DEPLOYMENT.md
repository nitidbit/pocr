# POCR Deployment Runbook — Test → Live-Study Migration → Full Deploy

End-to-end procedure for standing up POCR on a single Linux host, migrating a
live study from the legacy POC engine, and cutting over to production.

**Audience:** operator with root/`docker` access to the target box.
**Model:** everything runs via `docker compose`. The production stack is
`compose.yaml`; hot-reload dev is `compose.dev.yaml`.

> **Convention:** commands are run from the repo root on the Linux host unless a
> `cd` says otherwise. `$` = unprivileged shell; prefix `sudo` where your
> `docker` install requires it.

---

## 0. Prerequisites

| Requirement | Check |
|-------------|-------|
| Docker Engine + Compose v2 | `docker compose version` |
| Git checkout of this repo on the box | `git rev-parse HEAD` |
| Rust toolchain (only if running migrate/sync outside Docker) | `cargo --version` (see §3) |
| TLS cert + key for the server name | see §2.2 |
| Network reachability to legacy Redis (migration source) | see §3.1 |
| Network reachability to Caboodle / Epic Clarity (sync source) | see §5 |

The production `backend` compose network is `internal: true` — no outbound
internet. Only `nginx` and `vouch` are internet-facing. Plan firewall rules for
443 (HTTPS) and 1443 (gRPC/TLS) inbound.

---

## 1. Services & binaries (what you are deploying)

| Service | Image | Role | Ports |
|---------|-------|------|-------|
| `nginx` | `pocr-nginx` | TLS termination, reverse proxy | 443, 1443 |
| `vouch` | `vouch-proxy:0.48.0` | OIDC auth proxy | 9090 (internal) |
| `app` | `pocr-app` | React SPA (nginx-served) | 8080 (internal) |
| `engine` | `pocr-engine` | axum HTTP + tonic gRPC backend | 3000, 50051 (internal) |
| `postgres` | `postgres:17-alpine` | Primary store | 5432 (internal) |
| `redis` | `pocr-redis` | Randomization block cache | 6379 (internal) |

Two **one-shot** binaries live in the same Rust workspace but are **not** part of
the long-running stack (the production image ships only `pocr-engine`):

- **`pocr-migrate`** — one-time ETL: legacy Redis → POCR Postgres + Redis.
  Idempotent, re-runnable via a persisted `migration-manifest.json`.
- **`pocr-sync`** — refreshes attached datasets from Caboodle. Run after
  migration and on a schedule thereafter.

See §3 for how to run these on the box.

---

## 2. One-time host setup

### 2.1 Clone / update

```bash
git clone <repo-url> pocr && cd pocr
# or, updating an existing checkout:
git fetch origin && git checkout main && git pull --ff-only
```

### 2.2 TLS certificates

`nginx` mounts `./certs:ro` and the engine reads `CERT_PATH=/etc/certs/ssl.pem`.
Place your cert and key in `./certs/`:

```bash
mkdir -p certs
cp /path/to/fullchain.pem certs/ssl.pem
cp /path/to/privkey.pem   certs/ssl.key
chmod 600 certs/ssl.key
```

> Match the cert CN/SAN to `NGINX_SERVER_NAME` (default `pocr-app.ucsf.edu`).

### 2.3 Secrets

`compose.yaml` reads four Docker secrets from `./secrets/` (each a single line,
no trailing newline). **The password embedded in the `*_url` files MUST match the
corresponding `*_password` file** — the engine reads full URLs; postgres/redis
read bare passwords.

```bash
mkdir -p secrets && chmod 700 secrets
PG_PW=$(openssl rand -base64 24 | tr -d '/+=' | head -c 32)
REDIS_PW=$(openssl rand -base64 24 | tr -d '/+=' | head -c 32)

printf '%s' "$PG_PW"    > secrets/postgres_password
printf '%s' "$REDIS_PW" > secrets/redis_password
printf 'postgres://pocr:%s@postgres:5432/pocr' "$PG_PW"  > secrets/database_url
printf 'redis://default:%s@redis:6379'          "$REDIS_PW" > secrets/redis_url
chmod 600 secrets/*
```

| File | Contents |
|------|----------|
| `secrets/database_url` | `postgres://pocr:<PG_PW>@postgres:5432/pocr` |
| `secrets/redis_url` | `redis://default:<REDIS_PW>@redis:6379` |
| `secrets/postgres_password` | `<PG_PW>` (bare) |
| `secrets/redis_password` | `<REDIS_PW>` (bare) |

### 2.4 Vouch (OIDC) config

`vouch` mounts `./vouch/config.yml:ro`. Populate it with your IdP client ID/secret,
callback URL (`https://<server_name>/auth`), and allowed domains per the
[vouch-proxy docs](https://github.com/vouch/vouch-proxy). Keep this file out of git.

`certs/`, `secrets/`, and `vouch/` are host-local and are **not** committed.

---

## 3. TEST — validate the build & stack before touching live data

Run these in order. Do not proceed to migration (§4) until all pass.

### 3.1 Backend test suite (host, no Docker)

```bash
cd backend
cargo build --workspace --locked
cargo clippy --workspace --all-targets   # expect zero warnings
cargo test  --workspace                   # expect 0 failures
cd ..
```

### 3.2 Frontend test suite

```bash
cd frontend
npm ci
npx vitest run        # expect all pass
npx tsc --noEmit      # expect 0 errors
npx biome check src   # expect clean
cd ..
```

### 3.3 Build production images

```bash
docker compose build          # builds nginx, app, engine, redis
docker compose config -q      # validate compose + secrets wiring parse
```

### 3.4 Smoke-test the stack on a throwaway volume

Bring the stack up and confirm every service reaches `healthy`, then verify the
engine health endpoints and the SPA:

```bash
docker compose up -d postgres redis
docker compose up -d engine
docker compose ps            # engine + deps should be (healthy)

# engine liveness/readiness (from inside the backend network):
docker compose exec engine wget -qO- http://localhost:3000/health/live
docker compose exec engine wget -qO- http://localhost:3000/health/ready

docker compose up -d          # bring up app, nginx, vouch
curl -k https://localhost/    # SPA should serve (or redirect to vouch/OIDC)
```

Tear the smoke test down (this **destroys** the test DB/cache volumes — do it
before the real migration so you start clean):

```bash
docker compose down -v
```

---

## 4. MIGRATION — convert a live study (legacy Redis → POCR)

`pocr-migrate` reads the **legacy** Redis directly (read-only), transforms records
against the real `pocr-core` models, and writes POCR Postgres rows + Redis block
state. It runs its own Postgres schema migrations first (`run_migrations`), and is
**idempotent** via `migration-manifest.json`.

### 4.1 Configuration (environment variables)

| Var | Required | Meaning |
|-----|----------|---------|
| `LEGACY_REDIS_URL` | yes | Source Redis (legacy POC engine), read-only |
| `POCR_REDIS_URL` | yes | Destination POCR Redis block cache |
| `DATABASE_URL` | no (defaults to local dev URL) | Destination POCR Postgres — **set this in prod** |
| `MIGRATION_MANIFEST` | no (default `migration-manifest.json`) | Idempotency manifest path |

> In production the Postgres/Redis passwords are the ones from §2.3. Build the
> URLs to point at the **running compose services** (`postgres`, `redis`) or at
> host-mapped ports, depending on where you run the binary (see §4.3).

### 4.2 Pre-flight

1. **Back up** the legacy Redis (source) and the fresh POCR Postgres (destination):
   ```bash
   docker compose exec postgres pg_dump -U pocr pocr > backup-pre-migration-$(date +%F).sql
   ```
2. Ensure the production stack's `postgres` and `redis` are **up and healthy**
   (the engine may be up too — migration only writes rows/block state):
   ```bash
   docker compose up -d postgres redis
   docker compose ps
   ```
3. Confirm read connectivity to the legacy Redis from the box:
   ```bash
   redis-cli -u "$LEGACY_REDIS_URL" ping   # -> PONG
   ```

### 4.3 Run the migration

Because the production image ships only `pocr-engine`, run the one-shot binary
one of two ways:

**Option A — build & run the binary on the host** (simplest if Rust is installed):

```bash
cd backend
export LEGACY_REDIS_URL='redis://<legacy-host>:6379'
export POCR_REDIS_URL="redis://default:$(cat ../secrets/redis_password)@localhost:6379"
export DATABASE_URL="postgres://pocr:$(cat ../secrets/postgres_password)@localhost:5432/pocr"
export MIGRATION_MANIFEST=../migration-manifest.json

# Requires postgres/redis ports published to the host. compose.yaml keeps them
# internal — either use compose.dev.yaml's published ports for a staging run,
# or run Option B against the internal network.
cargo run --release -p pocr-migrate
cd ..
```

**Option B — run inside the backend network via a one-off container** (no host
port exposure needed; uses service DNS names `postgres`/`redis`):

```bash
docker compose run --rm --no-deps \
  -e LEGACY_REDIS_URL='redis://<legacy-host>:6379' \
  -e POCR_REDIS_URL="redis://default:$(cat secrets/redis_password)@redis:6379" \
  -e DATABASE_URL="postgres://pocr:$(cat secrets/postgres_password)@postgres:5432/pocr" \
  -e MIGRATION_MANIFEST=/tmp/migration-manifest.json \
  --entrypoint sh engine -c \
  'echo "Option B requires a migrate binary in the image — see note below"'
```

> **Note (packaging gap):** the production `backend/Dockerfile` only copies
> `pocr-engine`. To use Option B, add `pocr-migrate`/`pocr-sync` to the builder
> `COPY` layer (or build a dedicated `tools` image target). Until then, use
> Option A. This is called out in §7.

### 4.4 Verify the migration

On success the binary logs `migration complete` with `studies`, `randomizations`,
`locators`, `blocks` counts and the manifest path. Cross-check the destination:

```bash
docker compose exec postgres psql -U pocr -d pocr -c \
  "SELECT count(*) FROM studies; SELECT count(*) FROM randomizations;"
```

Compare counts against the logged totals and against the legacy source. Re-running
`pocr-migrate` with the same manifest is safe (idempotent) — it resumes/skips
already-migrated records.

---

## 5. SYNC — refresh datasets

`pocr-sync` refreshes every study dataset attachment: it reads each attached
study's captured randomizations from POCR Postgres, calls the dataset's Caboodle
stored procedure, and publishes the result as a snapshot. Package, procedure, and
refresh details are in [backend/datasets/README.md](../backend/datasets/README.md).

### 5.1 Configuration

| Var | Required | Default |
|-----|----------|---------|
| `DATABASE_URL` | no | `postgres://pocr:pocr@localhost:5432/pocr` (set in prod) |
| `DATASETS_DIR` | yes | — (`/etc/pocr/datasets` in the compose tools service) |
| `DATASET_ENVIRONMENT` | yes | — (`production` or `nonproduction`) |
| `DATASET_REFRESH_CONCURRENCY` | no | `1` |
| `DATASET_PROCEDURE_TIMEOUT_SECS` | no | `300` |
| `CABOODLE_HOST` | yes | — |
| `CABOODLE_PORT` | no | `1433` |
| `CABOODLE_DATABASE` | yes | — |
| `CABOODLE_USERNAME` | yes | — |
| `CABOODLE_PASSWORD` | yes | — |
| `CABOODLE_TRUST_SERVER_CERTIFICATE` | no | `false` (`true` or `false`) |

Sync fails with a configuration error when `DATASETS_DIR` is unset.

Only SQL Server authentication is supported. Supply a dedicated service account
through `CABOODLE_USERNAME` and `CABOODLE_PASSWORD`. The account needs `EXECUTE`
on the dataset stored procedures, not table reads. Missing or blank required
values, invalid ports, and a trust setting other than `true` or `false` fail
configuration. Legacy AD environment variables are ignored.

Connections require TLS and by default validate the server certificate and
hostname. Install the issuing CA in the runtime trust store and use the
certificate's DNS hostname.

`CABOODLE_TRUST_SERVER_CERTIFICATE=true` accepts any server certificate, for
instances that present only SQL Server's self-signed fallback certificate
(`SSL_Self_Signed_Fallback`), which rotates on restart and never matches the host.
Traffic stays encrypted, but the server is not authenticated: anyone able to
intercept the connection can impersonate Caboodle and capture the service-account
password and procedure results. Sync logs a `WARN` on each connection while it is
set. Use it only on a trusted network path.

Inject credentials through the deployment secret manager; do not commit
credentials or place them in shell history.

Caboodle uses Tiberius with SQL authentication and native TLS; integrated
authentication features are disabled. PostgreSQL uses SQLx. Connection setup
has a 15-second timeout. Database errors expose categories and server error
codes, not raw driver messages that could contain patient data. The shared
logging setup suppresses Tiberius internal events even when `RUST_LOG` enables
TRACE, because the driver logs server messages and row values.

### 5.2 Run

The `migrate` tools service mounts the catalog at `/etc/pocr/datasets`. Supply the
remaining variables with `-e`:

```bash
docker compose run --rm --entrypoint pocr-sync \
  -e DATABASE_URL=... -e DATASET_ENVIRONMENT=production \
  -e CABOODLE_HOST=... -e CABOODLE_DATABASE=... \
  -e CABOODLE_USERNAME=... -e CABOODLE_PASSWORD=... \
  migrate
```

Schedule it (cron/systemd timer) to keep snapshots current.

### 5.3 Demographics cutover

1. The database owner deploys `backend/datasets/demographics/procedure.sql` and
   grants the sync account `EXECUTE` on `[pocr].[EncounterBalance]`.
2. Deploy. The migration drops `patient_demographics`, and the engine loads the
   `demographics` package.
3. The study owner attaches **Demographics** in the study's Datasets tab.
4. Run sync with `DATASET_ENVIRONMENT=production`. The report appears at the top of
   Monitor.

---

## 6. FULL DEPLOY — production cutover

With migration verified (§4.4) and datasets refreshed (§5):

```bash
docker compose up -d          # nginx, vouch, app, engine, postgres, redis
docker compose ps             # all services (healthy)
```

### 6.1 Post-deploy verification

```bash
# All healthy?
docker compose ps

# External HTTPS entry (through nginx → app):
curl -kI https://<server_name>/           # 200 or 302→OIDC

# Engine readiness through the stack:
docker compose exec engine wget -qO- http://localhost:3000/health/ready

# Tail logs for errors on startup:
docker compose logs --since 2m engine nginx vouch
```

### 6.2 Manual sign-off checklist

- [ ] Browser: `https://<server_name>` loads the console (UCSF-branded).
- [ ] OIDC login via vouch succeeds; `/api/me` returns your identity.
- [ ] Migrated study is visible and its arm/block state is correct.
- [ ] A test randomization allocates and persists (check Postgres + Redis).
- [ ] Epic CDS Hooks endpoint reachable over gRPC/TLS on 1443 (if used).

### 6.3 Rollback

```bash
docker compose down                     # stop stack (keeps volumes/data)
# restore Postgres if needed:
cat backup-pre-migration-YYYY-MM-DD.sql | \
  docker compose exec -T postgres psql -U pocr -d pocr
docker compose up -d
```

`docker compose down` **without** `-v` preserves `pg_data`/`redis_data`. Only use
`-v` when you intend to wipe all data.

---

## 7. Known gaps / operator notes

1. **Migrate/sync not in the production image.** `backend/Dockerfile` production
   target ships only `pocr-engine`. Run `pocr-migrate`/`pocr-sync` via Option A
   (§4.3) until a tools image or extra `COPY` layer is added.
2. **Internal network vs. host ports.** `compose.yaml` keeps `postgres`/`redis`
   ports internal. Running the ETL binaries on the host (Option A) requires either
   publishing those ports or running against the compose network (Option B).
3. **Secret sync invariant.** Regenerating any password means regenerating BOTH
   the `*_password` and the matching `*_url` file (§2.3). A mismatch = engine
   cannot authenticate.
4. **`backend/CLAUDE.md`** is gitignored; its service-name references are local
   only and do not affect deploys.
5. **Docker Hub namespace** is `andrewdrobinson/ucsf:pocr-*`. Confirm the registry
   before pushing images for institutional use.

---

## Quick reference

```bash
# Test
cd backend && cargo test --workspace && cd ../frontend && npx vitest run && cd ..
docker compose build && docker compose config -q

# Migrate (Option A, host binary)
cd backend && LEGACY_REDIS_URL=... POCR_REDIS_URL=... DATABASE_URL=... \
  cargo run --release -p pocr-migrate && cd ..

# Refresh datasets
docker compose run --rm --entrypoint pocr-sync -e DATABASE_URL=... \
  -e DATASET_ENVIRONMENT=production -e CABOODLE_HOST=... -e CABOODLE_DATABASE=... \
  -e CABOODLE_USERNAME=... -e CABOODLE_PASSWORD=... migrate

# Deploy
docker compose up -d && docker compose ps

# Rollback (data preserved)
docker compose down && docker compose up -d
```
