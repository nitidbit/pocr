# POCR task runner. Run `just` (or `just --list`) to see recipes.
#
# Recipes never `cd`: each tool is rooted with its own directory flag
# (`--manifest-path`, `--prefix`, `-f`) so the repo top stays the cwd.

set shell := ["bash", "-cu"]
# Load .env if present (engine reads DATABASE_URL / REDIS_URL from it).
set dotenv-load := true

backend := "backend"
frontend := "frontend"

# buildx builder for bake recipes. Empty = current builder. Override per call
# (`just builder=pocr-remote push` — overrides precede the recipe name) or set
# BUILDER in the environment.
builder := env_var_or_default("BUILDER", "")
_builder_flag := if builder == "" { "" } else { "--builder " + builder }

# Build platforms passed through to bake (multi-arch needs a push output).
platforms := env_var_or_default("PLATFORMS", "linux/amd64")

alias t := test
alias b := build
alias c := check
alias d := fe-dev

# List recipes (default when `just` is run with no arguments).
default:
    @just --list

# -- Backend (Rust) --

# Compile the whole workspace.
[group('backend')]
build:
    cargo build --manifest-path {{backend}}/Cargo.toml

# Run the workspace test suite; append a filter, e.g. `just test block`.
[group('backend')]
test *args:
    cargo test --manifest-path {{backend}}/Cargo.toml {{args}}

# Lint every crate, denying warnings.
[group('backend')]
clippy:
    cargo clippy --manifest-path {{backend}}/Cargo.toml --workspace -- -D warnings

# Format the workspace in place.
[group('backend')]
fmt:
    cargo fmt --all --manifest-path {{backend}}/Cargo.toml

# Run the engine locally (DATABASE_URL / REDIS_URL from .env or the default POC values).
[group('backend')]
run:
    cargo run --manifest-path {{backend}}/Cargo.toml -p pocr-engine

# -- Frontend (Vite/React) --

# Install frontend dependencies from the lockfile.
[group('frontend')]
fe-install:
    npm --prefix {{frontend}} ci

# Start the Vite dev server on port 3000.
[group('frontend')]
fe-dev:
    npm --prefix {{frontend}} run dev

# Production build (typechecks as part of the build).
[group('frontend')]
fe-build:
    npm --prefix {{frontend}} run build

# Run the frontend test suite; append a path for one file.
[group('frontend')]
fe-test *args:
    npm --prefix {{frontend}} run test -- {{args}}

# Biome lint + format, applying fixes.
[group('frontend')]
fe-fix:
    npm --prefix {{frontend}} run check

# -- Whole-repo gates --

# Apply every autofix: rust fmt + frontend biome.
[group('gate')]
fix: fmt fe-fix

# Read-only verification, as CI runs it: no files are modified.
[group('gate')]
check:
    cargo fmt --all --manifest-path {{backend}}/Cargo.toml --check
    cargo clippy --manifest-path {{backend}}/Cargo.toml --workspace -- -D warnings
    cargo test --manifest-path {{backend}}/Cargo.toml
    npm --prefix {{frontend}} run check:ci
    npm --prefix {{frontend}} run build

# -- Docker images (bake) --
#
# All bake recipes pass `-f docker-bake.hcl` so bake does not also load
# compose.yaml and abort on its required runtime vars. Set `builder` to target
# a remote buildx node (`just builder=pocr-remote push`), or use `push-remote`.

# Build image(s) into the local store. Append targets: `just bake engine`.
[group('docker')]
bake *targets:
    PLATFORMS={{platforms}} docker buildx bake -f docker-bake.hcl {{_builder_flag}} {{targets}}

# Build and push image(s) to the registry. Required for remote/multi-arch builds.
[group('docker')]
push *targets:
    PLATFORMS={{platforms}} docker buildx bake -f docker-bake.hcl {{_builder_flag}} --push {{targets}}

# Build + push on the pocr-remote builder (remote driver cannot load locally).
[group('docker')]
push-remote *targets:
    @just builder=pocr-remote push {{targets}}

# Show the resolved bake plan without building.
[group('docker')]
bake-print *targets:
    docker buildx bake -f docker-bake.hcl {{_builder_flag}} {{targets}} --print

# -- Local stack (dev compose) --

# Start the hot-reload dev stack (engine, frontend, postgres, redis).
[group('stack')]
up:
    docker compose -f compose.dev.yaml up --build

# Start the dev stack detached.
[group('stack')]
upd:
    docker compose -f compose.dev.yaml up --build -d

# Stop the dev stack and remove its containers.
[group('stack')]
down:
    docker compose -f compose.dev.yaml down

# Tail dev-stack logs; append a service name, e.g. `just logs engine`.
[group('stack')]
logs *service:
    docker compose -f compose.dev.yaml logs -f {{service}}
