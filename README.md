# POCR — Point of Care Randomization

Clinical trial randomization engine, developed at UCSF. POCR configures
randomization studies and randomizes subjects at the point of care, either
through a web console or in response to Epic CDS Hooks requests.

## Running POCR at your institution

Each site runs its own POCR server, installed from a
[release](../../releases) bundle. Start with the document for your role:

| You are… | Read |
|---|---|
| Deciding whether to adopt POCR, or planning the project | [overview.md](docs/overview.md), [feature-status.md](docs/feature-status.md) |
| Reviewing it for security or compliance | [security-concerns.md](docs/security-concerns.md) |
| The server admin installing and running it | [install.md](docs/install.md) |
| The Epic analyst connecting Epic | [epic-integration.md](docs/epic-integration.md) |
| On a study team setting up a trial | [study-setup.md](docs/study-setup.md) |
| Using an EHR other than Epic | [epic-dependency.md](docs/epic-dependency.md) |
| A developer changing POCR | This README, then [docs/dev/](docs/dev/) |

Each release bundle includes the documentation for that release in `docs/`.

## Architecture

| Service | Description | Ports |
|---------|-------------|-------|
| **engine** | Rust backend (axum HTTP + tonic gRPC), PostgreSQL storage, Redis block cache | 3000 (HTTP), 50051 (gRPC) |
| **app** | Vite/React SPA console, served by nginx | 8080 |
| **postgres** | Primary storage (studies, randomizations) | 5432 |
| **redis** | Randomization block cache | 6379 (dev host: 6380) |
| **nginx** | TLS termination and reverse proxy | 443, 1443 (gRPC) |
| **vouch** | OAuth/OIDC authentication proxy | 9090 |

The `backend` network is internal (no outbound internet); only `nginx` and
`vouch` sit on the internet-facing `frontend` network.

## Repository Layout

| Path | Contents |
|------|----------|
| `backend/` | Rust workspace — `apps/` (engine, migrate, sync) and `crates/` (core domain, infra adapters, HTTP/gRPC APIs) |
| `frontend/` | Vite/React SPA |
| `nginx/` | Reverse proxy config and templates |
| `redis/` | Redis image, config, and ACL entrypoint |

Each of `backend/` and `frontend/` has its own `CLAUDE.md` with build commands
and conventions.

## Development

Run the full stack with hot reload:

```bash
docker compose -f compose.dev.yaml up
```

This starts the engine (via `cargo watch`), PostgreSQL, Redis, and the frontend
dev server on port 5179.

## Releases and production

Sites install from a release bundle (`deploy/`: a Compose file, a settings
template, and the `pocr-init`, `pocr-backup`, and `pocr-upgrade` scripts, plus
`docs/`) that pulls prebuilt, signed images. See [docs/install.md](docs/install.md).

Cut a release with `scripts/release.sh` (or `scripts/release 1.2.3`). It tags the
current commit and pushes a `v*` tag, which runs
`.github/workflows/release.yml`: it builds the `release` group in
`docker-bake.hcl`, pushes the images to `ghcr.io/<owner>`, signs them, and
attaches the bundle to a GitHub Release whose notes link to the docs at that
tag. Docs change in the same commit as the code they describe, so each tag's
docs match its images.

The top-level `compose.yaml` is UCSF's deployment: it builds or pulls UCSF's
images and bind-mounts `nginx/` and `vouch/config.yml` from the checkout. It
requires TLS certificates under `./certs/` and these secret files under
`./secrets/` (each a single line):

| File | Contents |
|------|----------|
| `database_url` | Full Postgres URL, e.g. `postgres://pocr:<password>@postgres:5432/pocr` |
| `redis_url` | Full Redis URL, e.g. `redis://default:<password>@redis:6379` |
| `postgres_password` | The Postgres password on its own |
| `redis_password` | The Redis password on its own |

The engine reads the full URLs; `postgres` and `redis` read the bare passwords.
The password embedded in `database_url` / `redis_url` **must match** its
corresponding `postgres_password` / `redis_password` file; `deploy/pocr-init`
shows one way to generate them together.
