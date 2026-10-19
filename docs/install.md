# Installing and Running POCR

_Last updated: 2026-10-19_

This guide is for the server admin installing POCR at a site. It is self-contained: it covers what to arrange beforehand, the install itself, and running the server afterwards. For the wider picture (staffing, Epic build, study setup), see [overview.md](overview.md).

**Verified:** install steps 3–7 and the backup and restore drill were run on 2026-10-15 with locally built images, a self-signed certificate, a mock OIDC identity provider (console login, allow and deny), and a fake Epic client. `pocr-init` was run as root on a Linux Docker Engine and as a non-root user on macOS. Not yet run: step 2 and [Upgrading](#upgrading) against a published release (none exists yet), signature verification, and login against a real identity provider. The Epic side is in a separate guide (planned).

**Contents**

- [What you end up with](#what-you-end-up-with)
- [Before you start](#before-you-start): who you need, what to gather, two limitations that shape the install
- [Install](#install): steps 1–7
- [Running POCR](#running-pocr): backups, upgrading, day-to-day commands
- [Try it with the public CDS Hooks sandbox](#try-it-with-the-public-cds-hooks-sandbox)
- [Troubleshooting](#troubleshooting)

---

## What you end up with

One Linux host running six Docker containers, with only nginx exposed:

| URL                                    | Who uses it                    | Protected by                                                           |
| -------------------------------------- | ------------------------------ | ---------------------------------------------------------------------- |
| `https://<your-host>/`                 | Study staff (web console)      | Login through your identity provider (Vouch)                           |
| `https://<your-host>/api/...`          | The console, behind the scenes | Same login                                                             |
| `https://<your-host>/cds-services/...` | Epic                           | Address allow-list (`CDS_ALLOW_FROM`) and your firewall. **No login.** |

| Container    | Role                                                                                                                                           |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| **nginx**    | The only thing exposed to the network. Terminates TLS, sends `/cds-services` straight to the engine, and requires a login for everything else. |
| **vouch**    | Handles console login against your identity provider ([Vouch Proxy](https://github.com/vouch/vouch-proxy)).                                    |
| **engine**   | The randomization service and the console's API.                                                                                               |
| **app**      | The web console (static files).                                                                                                                |
| **postgres** | The system of record: studies, every assignment, the audit log.                                                                                |
| **redis**    | Cache, plus the in-progress permuted block for each study and stratum.                                                                         |

Each endpoint, who calls it, and when is listed in [overview.md](overview.md#endpoints).

POCR never calls Epic: Epic sends everything POCR needs inside each request. So POCR needs no Epic credentials and no outbound network access to Epic.

You install from a **release bundle**: `compose.yaml`, `.env.example`, and a setup script, `pocr-init`. The POCR images are pulled from the project's container registry, already built and signed; nothing is compiled on your server. If your policy requires building images yourself, see [Building the images yourself](#building-the-images-yourself).

**The database holds PHI.** Each assignment records the EHR patient ID, encounter CSN, the Epic user ID of the clinician, the study, the arm, and the time. The server, its data volumes, and its backups fall under your normal PHI handling.

---

## Before you start

### Who you need

| Role                              | What they provide                                                                                                        |
| --------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| **Server / platform admin** (you) | The Linux host with Docker; runs this guide; owns backups and patching.                                                  |
| **Network / firewall admin**      | DNS name, TLS certificate, and firewall rules so that Epic can reach the server and only Epic can reach `/cds-services`. |
| **Identity admin**                | Registers POCR as an OIDC application so study staff can log in to the console.                                          |
| **Epic Interconnect admin**       | The addresses Epic will call from, and the FHIR base URL of each Epic environment that will call POCR.                   |

### What to gather

- [ ] **Host:** a Linux x86-64 VM; 2 vCPU, 4 GB RAM, 40 GB disk is ample. Put it close, network-wise, to your Epic Interconnect servers: POCR answers in milliseconds, so network distance to Epic is the part of response time you control.
- [ ] **DNS name** for the server, e.g. `pocr.example.org`, and a fixed IP address for it to point at. Used throughout as `<your-host>`.
- [ ] **TLS certificate** and key for that name, from a CA that both Epic Interconnect and your users' browsers trust. Your internal CA is usually fine for an internal-only server. A server with a public DNS name can use a free Let's Encrypt certificate instead (step 4).
- [ ] **Reachability:** the console reachable only from your internal network or VPN (recommended) or from the internet; `/cds-services` reachable only from Epic Interconnect.
- [ ] **Identity provider registration** (Okta, Entra ID, Ping, Keycloak, Google, or any OIDC provider): client ID, client secret, and the authorize, token, and userinfo URLs. The identity admin will ask for the redirect URI, `https://<your-host>/auth`, and the scopes, `openid email profile`.
- [ ] **Who may log in:** the email addresses of the study staff who will use the console.
- [ ] **Epic Interconnect addresses:** the IP addresses or CIDR ranges Epic calls from.
- [ ] **Epic environments:** the FHIR base URL of each Epic environment that will call this server (it typically ends in `/api/FHIR/R4`), and whether each counts as `production` or `nonproduction`. Many sites run one POCR server for all Epic environments; others run a separate test server.
- [ ] **Backup destination:** where nightly database dumps go (off this host), and who tests restores.

### Two limitations that shape the install

These are tracked in [security-concerns.md](security-concerns.md); read it before go-live.

- **The CDS Hooks endpoint does not verify Epic's signed token yet** (S2). Anyone who can reach `/cds-services` can create randomizations, so it must be reachable only from Epic Interconnect. This install enforces that with an address allow-list in nginx, and you should enforce it in your firewall as well.
- **Console users are not restricted per study yet** (S1). Anyone who can log in can change any study, so login is limited to a named list of people.

---

## Install

### 1. Prepare the host

- Docker Engine 24+ with the Compose plugin (`docker compose version`).
- `openssl` and `curl`.
- Outbound HTTPS to `ghcr.io` and `quay.io` to pull images (or a registry mirror your platform team provides).
- Inbound TCP 443 from your users' network and from Epic Interconnect. Nothing else needs to be open, unless you use Let's Encrypt (step 4), which also needs TCP 80 from anywhere.
- The DNS name pointing at the host's fixed address. On a cloud VM, reserve a static address (on AWS, an Elastic IP) first: the default public address changes when the VM is stopped and started.

### 2. Download the release bundle

Releases are listed at `https://github.com/nitidbit/pocr/releases`. Pick the latest version and unpack it into the directory you will run POCR from; this guide uses `/srv/pocr`:

```sh
V=1.0.0                                             # the release you chose
BASE=https://github.com/nitidbit/pocr/releases/download/v$V
curl -fLO $BASE/pocr-$V.tar.gz
curl -fLO $BASE/pocr-$V.tar.gz.sha256
sha256sum -c pocr-$V.tar.gz.sha256                  # expect: OK
sudo mkdir -p /srv/pocr
sudo tar -xzf pocr-$V.tar.gz -C /srv/pocr --strip-components=1 --no-same-owner
cd /srv/pocr
```

Everything below runs from `/srv/pocr`.

**Optional: verify the image signatures.** Each image is signed by the project's release workflow with [Sigstore cosign](https://docs.sigstore.dev/). With cosign installed:

```sh
for i in engine app redis nginx; do
  cosign verify ghcr.io/nitidbit/pocr-$i:$V \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    --certificate-identity-regexp '^https://github.com/nitidbit/pocr/\.github/workflows/release\.yml@refs/tags/v'
done
```

Each image also carries an SBOM and build provenance (`docker buildx imagetools inspect ghcr.io/nitidbit/pocr-engine:$V --format '{{json .SBOM}}'`), for your vulnerability scanning.

### 3. Run `pocr-init`

```sh
sudo ./pocr-init
```

It is safe to re-run at any time; it never replaces a secret or volume that already exists. It:

- generates the database and Redis passwords and the engine's signing key into `secrets/`, with each password written into its connection URL at the same time so the two always match;
- sets ownership so only root and the engine's user (uid 1001) can read them;
- creates `.env` from `.env.example` and fills in Vouch's two session keys;
- creates the two data volumes, `pocr-postgres-data` and `pocr-redis-data`, outside Compose, so no Compose command can delete them (`docker compose down -v` skips external volumes);
- finishes with a list of what is still left to configure.

### 4. Install the TLS certificate

nginx serves the certificate from two files: `certs/ssl.pem` (the certificate followed by any intermediate certificates, the "full chain") and `certs/ssl.key` (its private key). The certificate must be trusted by Epic Interconnect (it is the TLS client when Epic calls POCR) and by your users' browsers.

Pick one of three ways to get it:

| Option                                                     | Use it for                                   | Needs                            |
| ---------------------------------------------------------- | -------------------------------------------- | -------------------------------- |
| [A. Your institution's CA](#option-a-your-institutions-ca) | Production, and any server Epic calls        | Your PKI or network team         |
| [B. Let's Encrypt](#option-b-lets-encrypt)                 | A demo or test server with a public DNS name | Inbound TCP 80 from the internet |
| [C. Self-signed](#option-c-self-signed-testing-only)       | A throwaway install that only you use        | Nothing                          |

#### Option A: your institution's CA

Most institutions issue server certificates through a PKI or network team, from a certificate signing request (CSR). Generate the key and CSR on the server, so the private key never leaves it:

```sh
H=pocr.example.org                       # your POCR_HOST
sudo openssl req -new -newkey rsa:2048 -nodes -subj "/CN=$H" -addext "subjectAltName=DNS:$H" \
  -keyout certs/ssl.key -out certs/pocr.csr
cat certs/pocr.csr
```

Send `certs/pocr.csr` through your normal certificate request process. If your team generates the key for you instead, ask for the key and certificate in PEM format. When the certificate comes back, put it in `certs/ssl.pem` with its intermediates after it (your team can tell you which ones; skip the root):

```sh
cat your-host.crt intermediate.crt | sudo tee certs/ssl.pem >/dev/null
sudo ./pocr-init                         # sets the files' permissions and shows the certificate's name and expiry
```

Note the expiry date. To renew, copy the new certificate over `certs/ssl.pem` (use `cp` or `tee`, not `mv`, so the running containers see the change) and run `sudo docker compose exec nginx nginx -s reload`.

#### Option B: Let's Encrypt

[Let's Encrypt](https://letsencrypt.org/) issues free certificates that browsers trust, for any server with a public DNS name. Check with your Epic team before relying on one for Epic traffic.

Before you start, the name must already point at this server, and resolve the same way from the internet:

- Create an `A` record for `<your-host>` with the server's public IPv4 address (on AWS, its Elastic IP), in whatever DNS service hosts your domain. If that service can proxy traffic (Cloudflare's orange cloud, for example), turn proxying off for this record: POCR must see callers' real addresses for `CDS_ALLOW_FROM` to work.
- From any machine, `dig +short <your-host>` should print that address. A new record can take a few minutes to appear.

Let's Encrypt checks that you control the name by connecting to it on **port 80 from the internet**, both when the certificate is issued and at each renewal (certificates last 90 days; renewal is automatic). Nothing listens on port 80 except certbot while it runs, but your firewall must allow it from anywhere. If that is not acceptable, use option A.

```sh
H=pocr.example.org                       # your POCR_HOST
sudo apt-get install -y certbot
sudo certbot certonly --standalone -d $H -m <your-emailaddress> --agree-tos --no-eff-email   # a real email address, for Let's Encrypt notices

sudo cp /etc/letsencrypt/live/$H/fullchain.pem certs/ssl.pem
sudo cp /etc/letsencrypt/live/$H/privkey.pem   certs/ssl.key
sudo ./pocr-init                         # sets the files' permissions
```

The certbot package installs a timer that renews the certificate before it expires. To have each renewal copied into place and loaded by nginx:

```sh
sudo tee /etc/letsencrypt/renewal-hooks/deploy/pocr >/dev/null <<'EOF'
#!/bin/sh
set -e
cp "$RENEWED_LINEAGE/fullchain.pem" /srv/pocr/certs/ssl.pem
cp "$RENEWED_LINEAGE/privkey.pem"   /srv/pocr/certs/ssl.key
cd /srv/pocr && docker compose exec -T nginx nginx -s reload
EOF
sudo chmod 755 /etc/letsencrypt/renewal-hooks/deploy/pocr
sudo certbot renew --dry-run             # checks that renewal works (does not run the hook)
```

#### Option C: self-signed (testing only)

Set `POCR_HOST` in `.env` first (step 5), then:

```sh
sudo ./pocr-init --self-signed
```

That makes a 30-day certificate for `POCR_HOST`. Browsers show a warning you have to click through, Epic will refuse it, and the [CDS Hooks sandbox](#try-it-with-the-public-cds-hooks-sandbox) usually cannot reach a server that uses one (use option B for a sandbox demo).

### 5. Fill in `.env`

Edit `/srv/pocr/.env`. Each setting is explained in the file; in summary:

| Variable                                | What to put there                                                                                                                                                                                                                                                                                                                                                                                                                |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POCR_IMAGE_REPO`, `POCR_VERSION`       | Already set by the release bundle. Change `POCR_VERSION` only to upgrade ([Upgrading](#upgrading)).                                                                                                                                                                                                                                                                                                                              |
| `POCR_HOST`                             | `<your-host>`.                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `FHIR_ENV_MAP`                          | Which Epic environments may randomize, and whether each counts as `production` or `nonproduction`. Format: `prefix=environment` pairs separated by `;`. A request is matched by the `fhirServer` URL Epic sends, which is that environment's FHIR base URL. Requests from an unlisted URL get an "Unrecognized environment" card and are not recorded. Non-production assignments are kept separate and can be purged per study. |
| `CDS_ALLOW_FROM`                        | Your Epic Interconnect addresses or CIDR ranges, separated by spaces or commas. Empty refuses every caller. Applied by nginx when it starts.                                                                                                                                                                                                                                                                                                                      |
| `POCR_ALLOWED_USERS`                    | Email addresses of the people who may log in to the console, comma-separated.                                                                                                                                                                                                                                                                                                                                                    |
| `OAUTH_*`                               | From your identity provider registration (below).                                                                                                                                                                                                                                                                                                                                                                                |
| `VOUCH_JWT_SECRET`, `VOUCH_SESSION_KEY` | Already generated by `pocr-init`. Leave them.                                                                                                                                                                                                                                                                                                                                                                                    |

**Why `CDS_ALLOW_FROM` matters.** It is the mitigation for S2 above. `FHIR_ENV_MAP` is not an access control: the caller writes the `fhirServer` field itself, so it only prevents accidents (test traffic landing in production data). Enforce the same restriction in your network firewall as well if you can: if nginx sees every request as coming from one Docker gateway address (common behind some load balancers), the allow-list cannot tell callers apart. Step 7 shows how to check.

**Who may log in.** Because of S1, list named people in `POCR_ALLOWED_USERS`. Admitting a whole email domain is possible (leave `POCR_ALLOWED_USERS` empty and set `POCR_ALLOWED_EMAIL_DOMAINS=example.org`) but not recommended until S1 is fixed.

**Identity provider.** Put the registration's client ID, client secret, and authorize, token, and userinfo URLs into the `OAUTH_*` settings. For Google, set `OAUTH_PROVIDER=google` and delete the three URL lines. [Vouch Proxy's documentation](https://github.com/vouch/vouch-proxy) covers other providers.

When you are done, run `sudo ./pocr-init` again: its final list should be empty.

### 6. Start

```sh
sudo docker compose pull
sudo docker compose up -d
sudo docker compose ps
```

All six services should be `Up`, and `app`, `engine`, `postgres`, `redis` should be `(healthy)` within about a minute. The engine creates and migrates its database schema on every start; there is no separate migration step.

### 7. Verify

From a machine that can reach the server:

```sh
H=https://<your-host>

# Console requires login: expect 302 to /login
curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' $H/

# API requires login, even with a forged identity header: expect 401 twice
curl -s -o /dev/null -w '%{http_code}\n' $H/api/v1/studies
curl -s -o /dev/null -w '%{http_code}\n' -H 'X-Vouch-User: someone@example.org' $H/api/v1/studies

# From an address NOT in CDS_ALLOW_FROM: expect 403
curl -s -o /dev/null -w '%{http_code}\n' $H/cds-services
```

From an address in `CDS_ALLOW_FROM`:

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

Check that nginx sees real client addresses: `sudo docker compose logs nginx | tail` should show the addresses you made those requests from. If every request shows the same private address, enforce `CDS_ALLOW_FROM` in your firewall instead (step 5).

Finally, open `https://<your-host>/` in a browser, log in, and create a draft study.

The install is done. Before the first live study, set up [backups](#backups) and run the restore drill once.

### Building the images yourself

For sites whose policy requires building from source, in place of pulling the published images. You need `git` and Docker Buildx; the engine build compiles Rust and takes several minutes.

```sh
git clone --branch v$V https://github.com/nitidbit/pocr.git pocr-src
cd pocr-src
RELEASE_REPO=pocr-local TAG=$V docker buildx bake -f docker-bake.hcl release --load
```

That produces `pocr-local/pocr-{engine,app,redis,nginx}:$V` in the local image store. Install from the bundle as above (the same files are in the checkout's `deploy/` directory), with `POCR_IMAGE_REPO=pocr-local` in `.env`, and skip `docker compose pull` (use `docker compose up -d --pull never`).

---

## Running POCR

### Backups

Postgres holds every assignment and the audit trail; for a trial, it is the record that proves allocation integrity. A Docker volume survives restarts and upgrades, but not a lost disk or host, so back it up nightly, **to storage off this host**, and test restores. The minimum:

```sh
sudo mkdir -p /srv/pocr/backups && sudo chmod 700 /srv/pocr/backups
```

```sh
# /etc/cron.d/pocr-backup — nightly at 02:15, keep 30 days
15 2 * * * root cd /srv/pocr && docker compose exec -T postgres pg_dump -U pocr -Fc pocr > /srv/pocr/backups/pocr-$(date +\%F).dump && find /srv/pocr/backups -name 'pocr-*.dump' -mtime +30 -delete
```

Then ship `/srv/pocr/backups` to your backup system. (`-T` matters: without it the dump is corrupted by terminal handling.) The dumps contain PHI.

**Restore drill** (run once before go-live, then on a schedule). This restores into a throwaway container and leaves production alone:

```sh
docker run -d --name pocr-restore-test -e POSTGRES_USER=pocr -e POSTGRES_PASSWORD=restoretest -e POSTGRES_DB=pocr postgres:18-alpine
sleep 10
docker exec -i pocr-restore-test pg_restore -U pocr -d pocr --no-owner < backups/pocr-YYYY-MM-DD.dump
docker exec pocr-restore-test psql -U pocr -tAc "select (select count(*) from studies), (select count(*) from randomizations)"
docker rm -f pocr-restore-test
```

The counts should match production.

**Redis** needs no backup. It keeps an append-only file on its volume, so it survives restarts. If its data is lost entirely, no assignment is lost: assignments are in Postgres, and repeat requests still return the original assignment. What is lost is each study's _in-progress_ permuted block. The next request starts a fresh block, so arm counts can drift by up to one block's worth of imbalance per stratum.

**Never run `docker compose down -v` out of habit.** The data volumes are external, so it leaves them alone here, but it is the habit that deletes volumes on other systems.

### Upgrading

Read the release notes first. Then back up, download the new bundle, and take its `compose.yaml` and `pocr-init`. Your `.env`, `secrets/`, `certs/`, and `backups/` are yours and are left alone:

```sh
cd /srv/pocr
sudo docker compose exec -T postgres pg_dump -U pocr -Fc pocr | sudo tee backups/pre-upgrade-$(date +%F).dump >/dev/null

V=1.1.0
N=~/pocr-$V                                         # unpack outside /srv/pocr first
mkdir -p $N && cd $N
curl -fLO https://github.com/nitidbit/pocr/releases/download/v$V/pocr-$V.tar.gz
curl -fLO https://github.com/nitidbit/pocr/releases/download/v$V/pocr-$V.tar.gz.sha256
sha256sum -c pocr-$V.tar.gz.sha256                  # expect: OK
tar -xzf pocr-$V.tar.gz --strip-components=1
cd /srv/pocr
sudo cp $N/compose.yaml $N/pocr-init .
diff .env.example $N/.env.example; sudo cp $N/.env.example .   # copy any new settings into .env
sudo sed -i "s/^POCR_VERSION=.*/POCR_VERSION=$V/" .env

sudo ./pocr-init
sudo docker compose pull
sudo docker compose up -d
```

Schema migrations run automatically when the new engine starts. To roll back, set `POCR_VERSION` back and run `docker compose up -d`; if the new version migrated the schema, restore the pre-upgrade dump as well.

Third-party images (`postgres:18-alpine`, Vouch) receive security patches under the same tag; `docker compose pull && docker compose up -d` picks them up.

**Postgres major versions** (18 to 19) change the on-disk format, so the new image will not start on the old volume. Treat it as planned maintenance: take a final dump, stop the stack, create a fresh volume, start the new version against it, restore the dump, verify, and only then retire the old volume.

### Day-to-day commands

| Task                               | Command                                                                   |
| ---------------------------------- | ------------------------------------------------------------------------- |
| Status                             | `docker compose ps`                                                       |
| Engine logs                        | `docker compose logs -f engine`                                           |
| Apply a change to `.env`           | `docker compose up -d` (recreates only what changed). `docker restart` and `docker compose restart` do not re-read `.env`. |
| Readiness (DB + Redis) from inside | `docker compose exec engine wget -qO- http://127.0.0.1:3000/health/ready` |
| Stop everything (data kept)        | `docker compose down`                                                     |

---

## Try it with the public CDS Hooks sandbox

The [CDS Hooks sandbox](https://sandbox.cds-hooks.org) is a free web page that plays the part of an EHR: it shows a synthetic patient and calls your CDS services the way Epic would. It is the quickest way to see your server return a card without any Epic work. To try it against POCR on a laptop instead, see [external-dev-guide.md](external-dev-guide.md#trying-the-cds-hooks-sandbox-locally).

The sandbox runs **in the browser**: the browser of whoever is using it makes the calls to POCR. So the server must be reachable from that browser, and its certificate must be one the browser trusts (step 4, option A or B). With a self-signed certificate the calls usually fail without any warning.

Use it only on a server with synthetic data and no Epic connection. These settings publish study names and let anyone on the allow-list create randomizations ([security-concerns.md](security-concerns.md) S2–S4).

### Set up the server

Add to `.env`:

```sh
FHIR_ENV_MAP=https://launch.smarthealthit.org/=nonproduction
CDS_ALLOW_FROM=<browser-ips>      # IPs of the browsers that run the sandbox. Or use `all`
CDS_CORS_ORIGINS=https://sandbox.cds-hooks.org
CDS_DISCOVERY_LIST_STUDIES=true
```

- `FHIR_ENV_MAP`: the sandbox's patients come from `launch.smarthealthit.org`.
- `CDS_ALLOW_FROM`: the sandbox's own servers never contact POCR; the calls come from the browser the sandbox page is open in. So list the public IP of each computer that will run it (`curl -s https://checkip.amazonaws.com` on that computer shows it). `all` admits anyone.
- `CDS_CORS_ORIGINS` lets the sandbox page read POCR's responses.
- `CDS_DISCOVERY_LIST_STUDIES` lists each open study as its own service. The sandbox cannot send the Epic locator POCR normally uses to find the study, so it calls `/cds-services/<study-id>` instead.

Then `sudo docker compose up -d`.

### Run it

1. Log in to `https://<your-host>/` and create a study (draft is fine), e.g. two arms, block randomization, key "patient ID".
2. In the same browser, open `https://sandbox.cds-hooks.org/?serviceDiscoveryURL=https://<your-host>/cds-services`. (Or use the gear menu → _Add CDS Services_ and enter that URL.)
3. On the Patient View tab you should see a "Randomized: <arm>" card from POCR. Reloading shows the same assignment for the same patient; _Change Patient_ gives a new assignment. In the Change Patient dialog, click _Save_ once and close the dialog with its ×: the dialog stays open after saving, and a second _Save_ clears the cards.
4. The sandbox also calls the Epic-style `randomization` service and shows its "No locator found" card. That is expected; turn it off under gear → _Configure CDS Services_.
5. In the console, set the data environment selector in the header to **Non-Production** and open the study's **Monitoring** tab. Each sandbox patient is listed under Recent Randomizations. **Purge Non-Production Data** on the same tab clears them.

If no POCR card appears:

| What you see | Cause |
|---|---|
| The sandbox cannot load the services at all | The certificate is not trusted by the browser, or the server is unreachable from it. Open `https://<your-host>/cds-services` in the same browser to see which. |
| `403` on `/cds-services` | The browser's public address is not in `CDS_ALLOW_FROM`. It changes when you switch networks. To see the list nginx is using: `sudo docker compose exec nginx cat /etc/nginx/snippets/cds-access.conf`. |
| A CORS error in the browser console | `CDS_CORS_ORIGINS` is not set, or `docker compose up -d` was not run after setting it. |
| Only the "No locator found" card | `CDS_DISCOVERY_LIST_STUDIES` is not `true`, or the study is closed. |
| "Unrecognized environment" card | `FHIR_ENV_MAP` does not include `https://launch.smarthealthit.org/`. |

### Afterwards

To return the server to normal operation, remove `CDS_CORS_ORIGINS` and `CDS_DISCOVERY_LIST_STUDIES`, restore `FHIR_ENV_MAP` and `CDS_ALLOW_FROM` to your Epic values, run `sudo docker compose up -d`, and purge the sandbox data from each study.

---

## Troubleshooting

| Symptom                                                                                   | Cause and fix                                                                                                                                                                                                                 |
| ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docker compose` fails: `set POCR_HOST in .env` (or another variable)                     | `.env` missing or incomplete (step 5). Run compose from `/srv/pocr`.                                                                                                                                                          |
| `docker compose` fails: `run pocr-init to set VOUCH_JWT_SECRET`                           | Run `sudo ./pocr-init`.                                                                                                                                                                                                       |
| `docker compose pull` or `cosign verify` fails: `UNAUTHORIZED: authentication required`   | The image repository is private. The project's published images are public; if you pull from a private mirror, run `sudo docker login <registry>` first (and `cosign login <registry>` for verification).                     |
| `docker compose` fails: `secret ... file ... not found` or a certificate bind mount fails | `secrets/` or `certs/` missing; run `sudo ./pocr-init` (step 3) and install the certificate (step 4).                                                                                                                         |
| `pocr-init` says `secrets/ is partly filled`                                              | Some secret files were deleted. See its message: Postgres keeps the password it was first started with.                                                                                                                       |
| `vouch` restarting                                                                        | `docker compose logs vouch` names the setting. Common: an `OAUTH_*` value missing, or `callback_url ... must be within a configured domains` (`POCR_HOST` mismatch).                                                          |
| Login loops back to the login page, or the IdP says "redirect URI mismatch"               | The IdP's registered redirect URI must be exactly `https://<POCR_HOST>/auth`.                                                                                                                                                 |
| Logged in but "not authorized" (Vouch 403 page)                                           | Your email isn't in `POCR_ALLOWED_USERS`. Change it, then `docker compose up -d`.                                                                                                                                             |
| nginx: `host not found in upstream "vouch:9090"`                                          | Vouch wasn't up yet; nginx restarts and recovers. If it persists, fix Vouch first.                                                                                                                                            |
| Engine exits: `password authentication failed`                                            | Postgres only reads its password when the volume is first initialized. If `secrets/` was regenerated after that, change the password inside Postgres to match `secrets/postgres_password` (`ALTER USER pocr PASSWORD '...'`). |
| Engine exits: `failed to read DATABASE_URL_FILE ... permission denied`                    | Secret file not readable by uid 1001. Re-run `sudo ./pocr-init` (as root), which fixes ownership.                                                                                                                             |
| CDS call returns "Unrecognized environment"                                               | `fhirServer` in the request doesn't start with a `FHIR_ENV_MAP` prefix. Check the engine log for the URL received.                                                                                                            |
| CDS call returns `403`                                                                    | The caller's address isn't in `CDS_ALLOW_FROM`. Change it, then `docker compose up -d`.                                                                                                                                       |
| Epic shows no card for an assigned patient                                                | Expected for every arm except the one assigned: POCR answers `412` when the request's locator belongs to a different arm. See the Epic integration guide (planned).                                                           |
