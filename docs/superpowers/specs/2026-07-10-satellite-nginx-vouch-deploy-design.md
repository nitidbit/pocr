# Satellite `deploy/` — mounted-cert nginx + vouch sidecar

**Date:** 2026-07-10
**Status:** Approved

## Goal & scope

Create a self-contained `cloud-randomization/satellite/deploy/` directory that runs
nginx + vouch the pocr way — **certs mounted from the host** (rotate by swapping files
+ `docker compose restart nginx`, no image rebuild) and **vouch as a compose sidecar** —
while leaving the running `service`, `redis`, and `app` (vite-sample) images/containers
behaviorally untouched.

The old `nginx/`, `vouch/`, and `compose.*.yml` files stay in place as reference. This is
prompted by an SSL cert rotation on the Ubuntu host `qccrndlap701`, where certs were
previously baked into the nginx image.

## Context (what is actually running on qccrndlap701)

```
satellite-app-1      andrewdrobinson/ucsf:vite-sample             80/tcp, 3000/tcp
satellite-nginx-1    andrewdrobinson/ucsf:cre-satellite-nginx     80,443,1443
satellite-service-1  andrewdrobinson/ucsf:cre-satellite-service   (API 30000, gRPC 50051)
satellite-redis-1    andrewdrobinson/ucsf:cre-satellite-redis     6379 (healthy)
```

- The **running** nginx config (`nginx/default.conf.template`) has **no vouch**.
- Redis auth is self-contained: password lives in the baked `users.acl`
  (`user default on >… ~* +@all`). No compose secret is required for redis.
- `service` gets `REDIS_PASS` from `.env.service`.
- A committed Google OAuth client_secret exists in `vouch/config/config.yaml` — reused
  per decision, flagged as still-exposed.

## Directory layout

```
satellite/deploy/
  compose.yaml              # full stack: nginx, vouch, app, service, redis
  .env                      # NGINX_SERVER_NAME, ports
  .env.app                  # copied from satellite root
  .env.service              # copied from satellite root
  .gitignore                # certs/
  certs/                    # ssl.pem, ssl.key — MOUNTED, gitignored
  nginx/
    Dockerfile              # nginx:1.29-alpine, COPY conf only (no certs)
    nginx.conf              # http{} with $connection_upgrade map (pocr style)
    snippets/
      proxy-common.conf
      vouch-proxy-headers.conf
      vouch-validate.conf
    templates/
      app.conf.template     # :443
      grpc.conf.template    # :1443
  vouch/
    config.yml              # Google OAuth (existing client_id/secret reused)
```

## Upstream topology (satellite's, not pocr's)

pocr's clean snippet structure, pointed at satellite's services:

| Route            | Upstream         | Auth                                   |
|------------------|------------------|----------------------------------------|
| `/`              | `app:3000`       | **vouch-protected**                    |
| `/api/`          | `service:30000`  | passthrough (vouch cookie forwarded)   |
| `/cds-services`  | `service:30000`  | passthrough (public — Epic CDS Hooks)  |
| `/debug`         | `service:30000`  | passthrough                            |
| gRPC `:1443`     | `service:50051`  | none (network-level)                   |

`/api`, `/cds-services`, `/debug` are **unauthenticated** (match pocr): vouch guards only
browser app routes; the vouch cookie is forwarded downstream so `service` can read
`X-Vouch-*` headers when present. CDS Hooks calls from Epic cannot perform interactive
OAuth, so `/cds-services` must stay public.

## Certs

`compose.yaml` mounts `./certs:/etc/nginx/certs:ro`. nginx `Dockerfile` has **no**
`COPY … certs` line. Rotate: drop new `ssl.pem`/`ssl.key` into `deploy/certs/`, then
`docker compose restart nginx`. `certs/` is gitignored.

## Vouch

`vouch/config.yml` = existing Google OAuth config: `client_id`/`client_secret` reused,
`cookie.domain: qccrndlap701.campus.net.ucsf.edu`, callback
`https://qccrndlap701.campus.net.ucsf.edu/auth`, `allowAllUsers: true`,
`domains: [localhost:8080]` retained. Sidecar on the `frontend` network, `expose: 9090`,
no host ports. nginx reaches it via `upstream vouch { server vouch:9090; }`.

> ⚠️ The Google client_secret is committed in the old repo's git history. Reuse is an
> explicit decision; it remains exposed and should be rotated eventually.

## compose.yaml

- **Networks:** `frontend` + `backend` (backend `internal: true`). nginx joins **both**;
  vouch + app on `frontend`; service + redis on `backend`.
- External `/cds-services` access is via **nginx:443** → `service:30000` over `backend`.
  `service`/`redis` on an internal network does not block CDS Hooks — those enter through
  nginx, not directly.
- **Env/secrets:** copied into `deploy/` (`.env.app`, `.env.service`) — self-contained.
  No redis secret (redis ACL is baked into its image).
- `service`, `redis`, `app` reference existing images unchanged:
  `andrewdrobinson/ucsf:cre-satellite-service`, `-redis`, `vite-sample`.
- nginx `depends_on` app + service + vouch.
- Host ports: `443:443`, `1443:1443` (drop plain `80` — TLS only).

## Deploy & rotation flow

1. Tear down old stack to free ports: `docker compose -f ../compose.prod.yml down`.
2. Bring up: `cd satellite/deploy && docker compose up -d --build`
   (only nginx builds locally; service/redis/app pull existing images).
3. Rotate certs later: replace `deploy/certs/ssl.{pem,key}` →
   `docker compose restart nginx`.

## Testing / verification

- `docker compose config` renders; `nginx -t` inside container validates templates.
- After up:
  - `curl -k https://qccrndlap701…/cds-services` → reaches service (unauth).
  - `curl -k https://qccrndlap701…/` → 302 redirect to `/login` (vouch).
  - gRPC port 1443 completes TLS handshake.
- Confirm the change is isolated: `service`/`redis` containers use the same images and
  behavior as before.
