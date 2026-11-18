# POCR Deployment Runbook — Test → Full Deploy

End-to-end procedure for standing up POCR on a single Linux host and deploying
to production.

**Audience:** operator with root/`docker` access to the target box. **Model:** everything runs via `docker compose`. The
production stack is
`compose.yaml`; hot-reload dev is `compose.dev.yaml`.

> **Convention:** commands are run from the repo root on the Linux host unless a
> `cd` says otherwise. `$` = unprivileged shell; prefix `sudo` where your
> `docker` install requires it.

---

## 0. Prerequisites

| Requirement                                                   | Check                      |
|---------------------------------------------------------------|----------------------------|
| Docker Engine + Compose v2                                    | `docker compose version`   |
| Git checkout of this repo on the box                          | `git rev-parse HEAD`       |
| Rust toolchain (only if running sync outside Docker)          | `cargo --version` (see §3) |
| TLS cert + key for the server name                            | see §2.2                   |
| Network reachability to Caboodle / Epic Clarity (sync source) | see §4                     |

The production `backend` compose network is `internal: true` — no outbound
internet. Only `nginx` and `vouch` are internet-facing. Plan firewall rules for
443 (HTTPS) and 1443 (gRPC/TLS) inbound.

---

## 1. Services & binaries (what you are deploying)

| Service    | Image                | Role                           | Ports                  |
|------------|----------------------|--------------------------------|------------------------|
| `nginx`    | `pocr-nginx`         | TLS termination, reverse proxy | 443, 1443              |
| `vouch`    | `vouch-proxy:0.48.0` | OIDC auth proxy                | 9090 (internal)        |
| `app`      | `pocr-app`           | React SPA (nginx-served)       | 8080 (internal)        |
| `engine`   | `pocr-engine`        | axum HTTP + tonic gRPC backend | 3000, 50051 (internal) |
| `postgres` | `postgres:18-alpine` | Primary store                  | 5432 (internal)        |
| `redis`    | `pocr-redis`         | Randomization block cache      | 6379 (internal)        |

One **one-shot** binary ships in the engine image but is **not** part of the
long-running stack: **`pocr-sync`** refreshes attached datasets from Caboodle. It
runs as the profiled compose service `sync`, on a schedule. See §4.2.

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

| File                        | Contents                                     |
|-----------------------------|----------------------------------------------|
| `secrets/database_url`      | `postgres://pocr:<PG_PW>@postgres:5432/pocr` |
| `secrets/redis_url`         | `redis://default:<REDIS_PW>@redis:6379`      |
| `secrets/postgres_password` | `<PG_PW>` (bare)                             |
| `secrets/redis_password`    | `<REDIS_PW>` (bare)                          |

### 2.4 Vouch (OIDC) config

`vouch` mounts `./vouch/config.yml:ro`. Populate it with your IdP client ID/secret,
callback URL (`https://<server_name>/auth`), and allowed domains per the
[vouch-proxy docs](https://github.com/vouch/vouch-proxy). Keep this file out of git.

`certs/`, `secrets/`, and `vouch/` are host-local and are **not** committed.

---

## 3. TEST — validate the build & stack before touching live data

Run these in order. Do not deploy (§5) until all pass.

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
before deploying so you start clean):

```bash
docker compose down -v
```

---

## 4. SYNC — refresh datasets

`pocr-sync` refreshes every study dataset attachment: it reads each attached
study's captured randomizations from POCR Postgres, calls the dataset's Caboodle
stored procedure, and publishes the result as a snapshot. Package, procedure, and
refresh details are in [backend/datasets/README.md](../backend/datasets/README.md).

### 4.1 Configuration

| Var                                 | Required | Default                                                                                                                |
|-------------------------------------|----------|------------------------------------------------------------------------------------------------------------------------|
| `DATABASE_URL_FILE`                 | no       | — (`/run/secrets/database_url` in the compose `sync` service); read and trimmed, takes precedence over `DATABASE_URL`  |
| `DATABASE_URL`                      | no       | `postgres://pocr:pocr@localhost:5432/pocr`; inside a container this fails as connection refused, so set one of the two |
| `DATASETS_DIR`                      | yes      | — (`/etc/datasets` in the compose `sync` service)                                                                      |
| `DATASET_ENVIRONMENT`               | yes      | — (`production` or `nonproduction`; `production` in the compose `sync` service)                                        |
| `DATASET_REFRESH_CONCURRENCY`       | no       | `1`                                                                                                                    |
| `DATASET_PROCEDURE_TIMEOUT_SECS`    | no       | `300`                                                                                                                  |
| `CABOODLE_HOST`                     | yes      | —                                                                                                                      |
| `CABOODLE_PORT`                     | no       | `1433`                                                                                                                 |
| `CABOODLE_DATABASE`                 | yes      | —                                                                                                                      |
| `CABOODLE_USERNAME`                 | yes      | —                                                                                                                      |
| `CABOODLE_PASSWORD`                 | yes      | —                                                                                                                      |
| `CABOODLE_TRUST_SERVER_CERTIFICATE` | no       | `false` (`true` or `false`)                                                                                            |

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
instances that present only SQL Server's self-signed fallback certificate (`SSL_Self_Signed_Fallback`), which rotates on
restart and never matches the host.
Traffic stays encrypted, but the server is not authenticated: anyone able to
intercept the connection can impersonate Caboodle and capture the service-account
password and procedure results. Sync logs a `WARN` on each connection while it is
set. Use it only on a trusted network path.

Inject credentials through the deployment secret manager; do not commit
credentials or place them in shell history. The compose `sync` service reads the
`CABOODLE_*` settings from `secrets/caboodle.env` (mode 600) and the PostgreSQL URL
from the `database_url` secret.

Caboodle uses Tiberius with SQL authentication and native TLS; integrated
authentication features are disabled. PostgreSQL uses SQLx. Connection setup
has a 15-second timeout. Database errors expose categories and server error
codes, not raw driver messages that could contain patient data. The shared
logging setup suppresses Tiberius internal events even when `RUST_LOG` enables
TRACE, because the driver logs server messages and row values.

### 4.2 Run

Sync runs as the one-shot compose service `sync` (profile `sync`), from the engine
image so it always matches the engine's migrations. It needs, beside `compose.yaml`:

| Path                     | Contents                                                                                                                                                                                                            |
|--------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `./datasets/`            | copy of `backend/datasets/`; mounted read-only at `/etc/datasets` on `engine` and `sync`. **Required**: new on upgrade; `engine` and `sync` refuse to start without it. Restart `engine` after changing it.         |
| `./secrets/caboodle.env` | `CABOODLE_HOST`, `CABOODLE_PORT`, `CABOODLE_DATABASE`, `CABOODLE_USERNAME`, `CABOODLE_PASSWORD`, and `CABOODLE_TRUST_SERVER_CERTIFICATE` as `NAME=value` lines; mode 600 (read by Compose on the host, not mounted) |
| `./secrets/database_url` | existing engine secret, readable by uid 1001 (§2.3)                                                                                                                                                                 |

Compose interpolates `caboodle.env`: `$NAME` in a value is replaced (an unset name
becomes empty) and `$$` becomes `$`. Single-quote any value containing `$`, e.g.
`CABOODLE_PASSWORD='pa$word'`.

```bash
docker compose run --rm sync                                         # production
docker compose run --rm -e DATASET_ENVIRONMENT=nonproduction sync
```

`sync` joins `backend` for PostgreSQL and the non-internal `caboodle` network for
egress to SQL Server. Exit status is nonzero when any refresh fails. Schedule it (cron/systemd timer) to keep snapshots
current, e.g.:

```cron
15 2 * * * cd /path/to/pocr && docker compose run --rm sync >> sync.log 2>&1
```

### 4.3 Demographics cutover

1. The database owner deploys `backend/datasets/demographics/procedure.sql` and
   grants the sync account `EXECUTE` on `[pocr].[EncounterBalance]`.
2. Copy `backend/datasets/` to `./datasets` on the host and deploy. The migration
   drops `patient_demographics`, and the engine loads the `demographics` package.
3. The study owner attaches **Demographics** in the study's Datasets tab.
4. Run sync with `DATASET_ENVIRONMENT=production`. The report appears at the top of
   Monitor.

---

## 5. FULL DEPLOY — production cutover

With datasets refreshed (§4):

```bash
docker compose up -d          # nginx, vouch, app, engine, postgres, redis
docker compose ps             # all services (healthy)
```

### 5.1 Post-deploy verification

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

### 5.2 Manual sign-off checklist

- [ ] Browser: `https://<server_name>` loads the console (UCSF-branded).
- [ ] OIDC login via vouch succeeds; `/api/me` returns your identity.
- [ ] A test randomization allocates and persists (check Postgres + Redis).
- [ ] Epic CDS Hooks endpoint reachable over gRPC/TLS on 1443 (if used).

### 5.3 Rollback

```bash
docker compose down                     # stop stack (keeps volumes/data)
# restore Postgres from a pg_dump backup if needed:
cat backup-YYYY-MM-DD.sql | \
  docker compose exec -T postgres psql -U pocr -d pocr
docker compose up -d
```

`docker compose down` **without** `-v` preserves `pg_data`/`redis_data`. Only use
`-v` when you intend to wipe all data.

---

## 6. Known gaps / operator notes

1. **Sync shares the engine image.** The production image ships `pocr-engine`
   and `pocr-sync`; the `sync` service runs the latter. Deploy it with the same
   image tag as `engine`: both apply migrations at startup, and a binary older
   than the database refuses to run.
2. **Sync on macOS.** On macOS, Tiberius's native TLS (Secure Transport)
   intermittently aborts the TDS TLS handshake with Caboodle (`connection closed
   via error`). Run sync on Linux, where OpenSSL connects reliably.
3. **Secret sync invariant.** Regenerating any password means regenerating BOTH
   the `*_password` and the matching `*_url` file (§2.3). A mismatch = engine
   cannot authenticate.

---

## Quick reference

```bash
# Test
cd backend && cargo test --workspace && cd ../frontend && npx vitest run && cd ..
docker compose build && docker compose config -q

# Refresh datasets (needs ./datasets and secrets/caboodle.env)
docker compose run --rm sync

# Deploy
docker compose up -d && docker compose ps

# Rollback (data preserved)
docker compose down && docker compose up -d
```
