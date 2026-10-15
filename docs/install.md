# Installing POCR

**Audience:** the server admin installing POCR for a health system. Read [overview.md](overview.md) first and work through its readiness checklist.
**Verified:** every step up to "Connect Epic" was run on 2026-10-14 against this repository's `compose.yaml`, using a self-signed certificate and a fake Epic client. Two exceptions: console login against a real identity provider, and Linux file ownership of the secrets (that run was on Docker Desktop for Mac). The Epic side is in a separate guide (planned).

---

## What you end up with

One Linux host running six containers from `compose.yaml`, with only nginx exposed:

| URL | Who uses it | Protected by |
|---|---|---|
| `https://<your-host>/` | Study staff (web console) | Login through your identity provider (Vouch) |
| `https://<your-host>/api/...` | The console, behind the scenes | Same login |
| `https://<your-host>/cds-services/...` | Epic | Your firewall / nginx allow-list (step 7). **No login.** |

Data lives in two Docker volumes: Postgres (system of record) and Redis (cache plus in-progress randomization blocks).

## 1. Prepare the host

- Linux x86-64 VM; 2 vCPU, 4 GB RAM, 40 GB disk is ample.
- Docker Engine 24+ with the Compose plugin (`docker compose version` should print v2.24.4 or later; older versions do not understand `!override` in step 3).
- `git` and `openssl`.
- Inbound TCP 443 from your users' network and from Epic Interconnect. Nothing else needs to be open. In particular, **do not open 1443**, the gRPC port (step 3 stops publishing it).
- DNS name pointing at the host, e.g. `pocr.example.org`. Used throughout as `<your-host>`.

Pick an install directory and keep it; this guide uses `/srv/pocr`.

```sh
sudo git clone https://github.com/<org>/pocr.git /srv/pocr   # use the published repository URL
cd /srv/pocr
```

Everything below runs from `/srv/pocr`. The files you create (`.env`, `secrets/`, `certs/`, `backups/`, `compose.override.yaml`) are in `.gitignore`, so `git pull` never touches them.

## 2. Create the database volumes

Create the volumes yourself, outside Compose, so no Compose command can delete them. (`docker compose down -v` skips volumes declared external. See [production-persistence.md](production-persistence.md).)

```sh
docker volume create pocr_pg_data
docker volume create pocr_redis_data
```

## 3. Create your site override file

`compose.yaml` carries UCSF's defaults. Put your changes in `compose.override.yaml`, which Compose merges in automatically:

```yaml
# /srv/pocr/compose.override.yaml
name: pocr          # fixes container, network, and volume names regardless of directory

services:
  nginx:
    # The gRPC listener has no backend yet; don't publish it.
    ports: !override
      - "443:443"

  # Images are built locally from this checkout. Renaming them keeps a stray
  # `docker compose pull` from replacing them with UCSF's published images.
  app:
    image: pocr-local/app:latest
  engine:
    image: pocr-local/engine:latest
  migrate:
    image: pocr-local/engine:latest
  redis:
    image: pocr-local/redis:latest

volumes:
  pg_data:
    external: true
    name: pocr_pg_data
  redis_data:
    external: true
    name: pocr_redis_data
```

## 4. Generate secrets

Each file holds one value. Each password appears twice: on its own (read by Postgres or Redis), and inside a connection URL (read by the engine). The two copies must match, so generate both together:

```sh
sudo mkdir -p secrets && sudo chmod 700 secrets
PG_PW=$(openssl rand -hex 24); REDIS_PW=$(openssl rand -hex 24)
printf '%s' "$PG_PW"    | sudo tee secrets/postgres_password >/dev/null
printf '%s' "$REDIS_PW" | sudo tee secrets/redis_password    >/dev/null
printf 'postgres://pocr:%s@postgres:5432/pocr' "$PG_PW"  | sudo tee secrets/database_url >/dev/null
printf 'redis://default:%s@redis:6379' "$REDIS_PW"       | sudo tee secrets/redis_url    >/dev/null
openssl rand -hex 32 | sudo tee secrets/jwt_secret >/dev/null
sudo chmod 644 secrets/*
unset PG_PW REDIS_PW
```

Why `644` on the files: Compose (outside Swarm) bind-mounts secret files into containers with their host ownership and mode, and the engine runs as uid 1001, not root. The `700` directory is what keeps other host users out. If you'd rather use `600`, `chown 1001` the two URL files.

Use hex passwords (as above). Characters such as `@`, `/`, or `:` would have to be URL-escaped inside the connection URLs.

## 5. Install the TLS certificate

nginx expects exactly these two files:

```sh
sudo mkdir -p certs
sudo cp /path/to/your-host.crt certs/ssl.pem    # full chain: server cert first, then intermediates
sudo cp /path/to/your-host.key certs/ssl.key
sudo chmod 644 certs/ssl.pem && sudo chmod 640 certs/ssl.key
```

The certificate must be trusted by Epic Interconnect (it is the TLS client when Epic calls POCR) and by your users' browsers.

For a throwaway test install only, a self-signed certificate works:

```sh
sudo openssl req -x509 -newkey rsa:2048 -nodes -days 30 \
  -subj "/CN=<your-host>" -addext "subjectAltName=DNS:<your-host>" \
  -keyout certs/ssl.key -out certs/ssl.pem
```

## 6. Configure the environment (`.env`)

```sh
sudo tee .env >/dev/null <<EOF
NGINX_SERVER_NAME=<your-host>
FHIR_ENV_MAP=https://<epic-prd-fhir-base>/=production;https://<epic-tst-fhir-base>/=nonproduction
JWT_SECRET=$(sudo cat secrets/jwt_secret)
EOF
sudo chmod 600 .env
```

| Variable | Required | What it does |
|---|---|---|
| `NGINX_SERVER_NAME` | Yes (defaults to UCSF's host name) | The host name nginx answers for. |
| `FHIR_ENV_MAP` | Yes; Compose refuses to start without it | Which EHR environments may randomize, and whether each counts as `production` or `nonproduction`. Format: `prefix=environment` pairs separated by `;`. A request is matched by the `fhirServer` URL Epic sends, which is that Epic environment's FHIR base URL (ask your Interconnect admin; it typically ends in `/api/FHIR/R4`). Requests from an unlisted URL get an "Unrecognized environment" card and are not recorded. Non-production assignments are kept separate and can be purged per study. |
| `JWT_SECRET` | Yes | Signs the console's study-sharing links. |
| `CDS_CORS_ORIGINS` | No; leave unset | Only for a demo server used with the public CDS Hooks sandbox (see the sandbox section below). Epic does not need it. |
| `CDS_DISCOVERY_LIST_STUDIES` | No; leave unset (`false`) | Same: demo servers only. Lists every open study, by name, to anyone who can reach `/cds-services`. |

`FHIR_ENV_MAP` is a safety net against test traffic landing in production data, not an access control: the caller writes the `fhirServer` field itself. Access control comes from step 7.

## 7. Restrict who can call `/cds-services`

POCR does not yet verify the signed token Epic attaches to each CDS Hooks call ([security-concerns.md](security-concerns.md) S2). Until it does, allow only your Epic Interconnect servers. Edit `nginx/snippets/cds-access.conf`:

```nginx
allow 10.20.30.40;      # Interconnect PRD
allow 10.20.31.0/24;    # Interconnect non-prod
deny  all;
```

This applies only to `/cds-services`; the console is unaffected. After starting the stack (step 9), confirm nginx sees real client addresses: make a request and check `docker compose logs nginx`. If every request appears to come from one Docker gateway address (common behind some proxies and on Docker Desktop), enforce the restriction in your network firewall instead.

You can apply the same restriction to the whole site at your network edge if the console is internal-only.

## 8. Configure console login (Vouch)

Edit `vouch/config.yml` on the host. It ships with UCSF's settings, which will not work for you. Every key below must change:

```yaml
vouch:
  # Who may log in. Prefer an explicit list of people (see security-concerns.md S1:
  # any logged-in user can currently change any study).
  whiteList:
    - pi@example.org
    - coordinator@example.org
  # ...or, less strictly, everyone in an email domain:
  # domains:
  #   - example.org

  cookie:
    secure: true
    domain: <your-host>
    name: VouchCookie

  # Without these, Vouch makes up new keys at every restart and logs everyone out.
  # Generate each with: openssl rand -hex 32
  jwt:
    secret: <random>
  session:
    key: <random>

  headers:
    claims:
      - name
      - email
      - picture

oauth:
  # Any OIDC provider (Okta, Entra ID, Ping, Keycloak, ...):
  provider: oidc
  client_id: <from your identity admin>
  client_secret: <from your identity admin>
  auth_url: https://<idp>/oauth2/v1/authorize
  token_url: https://<idp>/oauth2/v1/token
  user_info_url: https://<idp>/oauth2/v1/userinfo
  callback_urls:
    - https://<your-host>/auth
  scopes:
    - openid
    - email
    - profile
```

Ask your identity admin to register an application with redirect URI `https://<your-host>/auth`. For Google, set `provider: google` and drop the three URLs. Vouch's own documentation covers other providers (`github.com/vouch/vouch-proxy`, `config/config.yml_example_*`).

This file now holds a client secret: `sudo chmod 640 vouch/config.yml`, and do not commit it back to any repository.

## 9. Build and start

```sh
sudo docker compose build        # first build compiles the Rust engine; allow several minutes
sudo docker compose up -d
sudo docker compose ps
```

All six services should be `Up`, and `app`, `engine`, `postgres`, `redis` should be `(healthy)` within about a minute. nginx can restart once or twice on first start while Vouch comes up; that is harmless.

The engine creates and migrates its database schema on every start; there is no separate migration step. Ignore the `migrate` service in `compose.yaml`; it imports data from UCSF's previous system and is not needed elsewhere.

## 10. Verify

From a machine that can reach the server:

```sh
H=https://<your-host>

# Console requires login: expect 302 to /login
curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' $H/

# API requires login, even with a forged identity header: expect 401 twice
curl -s -o /dev/null -w '%{http_code}\n' $H/api/v1/studies
curl -s -o /dev/null -w '%{http_code}\n' -H 'X-Vouch-User: someone@example.org' $H/api/v1/studies
```

From an address in your `/cds-services` allow-list (or temporarily from anywhere, before you tighten step 7):

```sh
# Service discovery: expect one service with id "randomization"
curl -s $H/cds-services

# An Epic-shaped call with no study locator: expect a "No locator found" card.
# Replace the fhirServer value with a nonproduction prefix from your FHIR_ENV_MAP.
curl -s -H 'content-type: application/json' $H/cds-services/randomization -d '{
  "hook": "patient-view",
  "hookInstance": "00000000-0000-0000-0000-000000000001",
  "fhirServer": "https://<epic-tst-fhir-base>/api/FHIR/R4",
  "context": {"patientId": "test-patient", "userId": "Practitioner/test"},
  "prefetch": {"patient": {"resourceType": "Patient", "id": "test-patient"}}
}'
```

An "Unrecognized environment" card instead means the `fhirServer` you sent does not start with any prefix in `FHIR_ENV_MAP`.

From an address *outside* the allow-list, `curl $H/cds-services` should return `403`.

Finally, open `https://<your-host>/` in a browser, log in, and create a draft study.

## 11. Back up the database

Postgres holds every assignment and the audit trail; for a trial, it is the record that proves allocation integrity. Back it up nightly, **to storage off this host**, and test restores. [production-persistence.md](production-persistence.md) explains the reasoning; the minimum:

```sh
# /etc/cron.d/pocr-backup — nightly at 02:15, keep 30 days
15 2 * * * root cd /srv/pocr && docker compose exec -T postgres pg_dump -U pocr -Fc pocr > /srv/pocr/backups/pocr-$(date +\%F).dump && find /srv/pocr/backups -name 'pocr-*.dump' -mtime +30 -delete
```

Then ship `/srv/pocr/backups` to your backup system. (`-T` matters: without it the dump is corrupted by terminal handling.)

**Restore drill** (run once before go-live, then on a schedule). This restores into a throwaway container and leaves production alone:

```sh
docker run -d --name pocr-restore-test -e POSTGRES_USER=pocr -e POSTGRES_PASSWORD=restoretest -e POSTGRES_DB=pocr postgres:18-alpine
sleep 10
docker exec -i pocr-restore-test pg_restore -U pocr -d pocr --no-owner < backups/pocr-YYYY-MM-DD.dump
docker exec pocr-restore-test psql -U pocr -tAc "select (select count(*) from studies), (select count(*) from randomizations)"
docker rm -f pocr-restore-test
```

The counts should match production.

**Redis** needs no backup. It keeps an append-only file on its volume, so it survives restarts. If its data is lost entirely, no assignment is lost: assignments are in Postgres, and repeat requests still return the original assignment. What is lost is each study's *in-progress* permuted block. The next request starts a fresh block, so arm counts can drift by up to one block's worth of imbalance per stratum.

**Never run `docker compose down -v` out of habit**, and treat a Postgres major-version change (18 to 19) as a planned dump-and-restore migration ([production-persistence.md](production-persistence.md), Gap 4).

## 12. Updating

```sh
cd /srv/pocr
sudo docker compose exec -T postgres pg_dump -U pocr -Fc pocr > backups/pre-update-$(date +%F).dump
sudo git pull
sudo docker compose build
sudo docker compose up -d
```

Schema migrations run automatically when the new engine starts. Before pulling, check whether `vouch/config.yml` or `nginx/` changed upstream: `git pull` will stop if your local edits conflict, and you merge by hand.

## Day-to-day commands

| Task | Command |
|---|---|
| Status | `docker compose ps` |
| Engine logs | `docker compose logs -f engine` |
| Restart after editing nginx config | `docker compose restart nginx` |
| Restart after editing Vouch config | `docker compose restart vouch` |
| Readiness (DB + Redis) from inside | `docker compose exec engine wget -qO- http://localhost:3000/health/ready` |
| Stop everything (data kept) | `docker compose down` |

---

## Try it with the public CDS Hooks sandbox

The [CDS Hooks sandbox](https://sandbox.cds-hooks.org) is a free web page that plays the part of an EHR: it shows a synthetic patient and calls your CDS services the way Epic would. It is the quickest way to see POCR return a card without any Epic work.

The sandbox runs **in your browser**, and the browser makes the calls to POCR. So it can reach a POCR running on your own laptop at `http://localhost:3000` (Chrome and Firefox allow that from an HTTPS page; Safari does not), or a server on your internal network that your browser can reach. It cannot reach anything your browser cannot.

POCR needs two settings for the sandbox, both off by default:

- `CDS_CORS_ORIGINS=https://sandbox.cds-hooks.org` lets the sandbox page read POCR's responses.
- `CDS_DISCOVERY_LIST_STUDIES=true` lists each open study as its own service. The sandbox cannot send the Epic locator POCR normally uses to find the study, so it calls `/cds-services/<study-id>` instead.

The sandbox's patients come from `https://launch.smarthealthit.org/`, which must be in `FHIR_ENV_MAP` as `nonproduction`.

**On a developer laptop** (`compose.dev.yaml` sets all three by default):

1. `docker compose -f compose.dev.yaml up`
2. Open `http://localhost:5179`, create a study (draft is fine), e.g. two arms, block randomization, key "patient ID".
3. Open `https://sandbox.cds-hooks.org/?serviceDiscoveryURL=http://localhost:3000/cds-services` in Chrome. (Or use the gear menu → *Add CDS Services* and enter that URL.)
4. On the Patient View tab you should see a "Randomized: <arm>" card from POCR. Reloading shows the same assignment for the same patient; *Change Patient* gives a new assignment.

The sandbox also calls the Epic-style `randomization` service and shows its "No locator found" card. That is expected. Turn it off under gear → *Configure CDS Services*.

**On a demo server** (HTTPS, reachable by whoever will use the sandbox), add to `.env`:

```sh
FHIR_ENV_MAP=https://launch.smarthealthit.org/=nonproduction
CDS_CORS_ORIGINS=https://sandbox.cds-hooks.org
CDS_DISCOVERY_LIST_STUDIES=true
```

and leave `nginx/snippets/cds-access.conf` at `allow all;`. Do this only on a server with synthetic data and no Epic connection: it publishes study names and lets anyone create randomizations ([security-concerns.md](security-concerns.md) S2–S4).

---

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `docker compose up` fails: `set FHIR_ENV_MAP in .env` or `set JWT_SECRET in .env` | `.env` missing or incomplete (step 6). Run compose from `/srv/pocr`. |
| `vouch` restarting, log says `oauth.client_id not found` | `vouch/config.yml` not filled in (step 8). |
| Everyone is logged out after each restart | `jwt.secret` / `session.key` not set in `vouch/config.yml`. |
| Login loops back to the login page, or the IdP says "redirect URI mismatch" | `cookie.domain`, `callback_urls`, and the IdP's registered redirect URI must all use `<your-host>` exactly. |
| Logged in but "not authorized" | Your email isn't in `whiteList` / `domains`. |
| nginx: `host not found in upstream "vouch:9090"` | Vouch wasn't up yet; nginx restarts and recovers. If it persists, fix Vouch first. |
| Engine exits: `password authentication failed` | Password in `secrets/database_url` differs from `secrets/postgres_password`. Postgres only reads its password when the volume is first initialized: if you regenerate secrets later, change the password inside Postgres too (`ALTER USER pocr PASSWORD '...'`). |
| Engine exits: `failed to read DATABASE_URL_FILE ... permission denied` | Secret file not readable by uid 1001 (step 4). |
| CDS call returns "Unrecognized environment" | `fhirServer` in the request doesn't start with a `FHIR_ENV_MAP` prefix. Check the engine log for the URL received. |
| CDS call returns `403` | The caller's IP isn't in `nginx/snippets/cds-access.conf`. |
| Epic shows no card for an assigned patient | Expected for every arm except the one assigned: POCR answers `412` when the request's locator belongs to a different arm. See the Epic integration guide (planned). |
