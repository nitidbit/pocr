# Installing and Running POCR

_Last updated: 2026-10-26_

This guide is for the server admin installing POCR at a site. Connecting Epic is covered in [epic-integration.md](epic-integration.md), and setting up studies in [study-setup.md](study-setup.md). [overview.md](overview.md) gives the wider picture.

**Verified:** release v0.1.0 was installed with this procedure on 2026-10-16 (Ubuntu 26.04 on AWS EC2, published images, Let's Encrypt certificate, Google login, the CDS Hooks sandbox). The backup and restore drill was run on 2026-10-15 with locally built images. Not yet run against a published release: the `pocr-init --csr` and `--letsencrypt` options, `pocr-backup`, `pocr-upgrade`, and image signature verification.

**Contents**

- [What you end up with](#what-you-end-up-with)
- [Before you start](#before-you-start)
- [Install](#install): six steps
- [Running POCR](#running-pocr): commands, upgrading, backups, certificate renewal
- [Try it with the public CDS Hooks sandbox](#try-it-with-the-public-cds-hooks-sandbox)
- [Verifying the images, or building them yourself](#verifying-the-images-or-building-them-yourself)
- [Troubleshooting](#troubleshooting)

---

## What you end up with

One Linux host running six Docker containers, with only nginx exposed on port 443:

| URL                                    | Who uses it               | Protected by                                                           |
| -------------------------------------- | ------------------------- | ---------------------------------------------------------------------- |
| `https://<your-host>/`, `/api/...`     | Study staff (web console) | Login through your identity provider (Vouch)                           |
| `https://<your-host>/cds-services/...` | Epic                      | Address allow-list (`CDS_ALLOW_FROM`) and your firewall. **No login.** |

| Container    | Role                                                                                                   |
| ------------ | ------------------------------------------------------------------------------------------------------ |
| **nginx**    | Terminates TLS, sends `/cds-services` straight to the engine, and requires a login for everything else. |
| **vouch**    | Console login against your identity provider ([Vouch Proxy](https://github.com/vouch/vouch-proxy)).    |
| **engine**   | The randomization service and the console's API.                                                       |
| **app**      | The web console (static files).                                                                        |
| **postgres** | The system of record: studies, every assignment, the audit log.                                        |
| **redis**    | Cache, plus the in-progress permuted block for each study and stratum.                                 |

POCR never calls Epic: each request carries everything POCR needs. It needs no Epic credentials and no outbound access to Epic.

You install from a **release bundle**: `compose.yaml`, `.env.example`, and three scripts, `pocr-init`, `pocr-backup`, and `pocr-upgrade`. The images are pulled already built and signed; nothing is compiled on your server.

**The database holds PHI.** Each assignment records the EHR patient ID, encounter CSN, the clinician's Epic user ID, the study, the arm, and the time. The server, its volumes, and its backups fall under your normal PHI handling.

---

## Before you start

Gather these. The person who usually provides each is in brackets.

- [ ] **Host** [you]: Linux x86-64 VM with Docker Engine 24+ and the Compose plugin, `openssl`, and `curl`. 2 vCPU, 4 GB RAM, 40 GB disk is ample. Put it close, network-wise, to your Epic Interconnect servers.
- [ ] **DNS name** [network admin], e.g. `pocr.example.org`, pointing at a fixed IP address (on AWS, an Elastic IP). Used below as `<your-host>`.
- [ ] **Firewall** [network admin]: inbound TCP 443 from your users' network and from Epic Interconnect; `/cds-services` reachable only from Epic. Outbound HTTPS to `ghcr.io` and `quay.io` (or a registry mirror).
- [ ] **TLS certificate** [PKI or network team]: trusted by both Epic Interconnect and your users' browsers. Step 3 covers your institution's CA, Let's Encrypt, and self-signed.
- [ ] **Identity provider registration** [identity admin]: an OIDC application (Okta, Entra ID, Ping, Keycloak, Google, ...) with redirect URI `https://<your-host>/auth` and scopes `openid email profile`. You need its client ID, client secret, and authorize, token, and userinfo URLs.
- [ ] **Who may log in** [study team]: email addresses of the study staff who will use the console.
- [ ] **Epic details** [Epic Interconnect admin]: the IP addresses or CIDR ranges Epic calls from, and the FHIR base URL of each Epic environment that will call POCR (typically ending in `/api/FHIR/R4`), marked `production` or `nonproduction`.
- [ ] **Backup destination** [you]: storage off this host for nightly database dumps.

**Two limitations shape this install.** Both are tracked in [security-concerns.md](security-concerns.md); read it before go-live.

- **POCR does not yet verify Epic's signed token** (S2). Anyone who can reach `/cds-services` can create randomizations. This install limits it to the addresses in `CDS_ALLOW_FROM`; enforce the same in your firewall.
- **Console users are not yet restricted per study** (S1). Anyone who can log in can change any study, so login is limited to named people.

---

## Install

### 1. Download the release bundle

Pick the latest release from `https://github.com/nitidbit/pocr/releases` and unpack it into `/srv/pocr`:

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

Everything below runs from `/srv/pocr`. To check the image signatures as well, see [Verifying the images](#verifying-the-images-or-building-them-yourself).

### 2. Run `pocr-init` and fill in `.env`

```sh
sudo ./pocr-init
```

`pocr-init` generates the database and Redis passwords and the engine's signing key in `secrets/`, creates `.env` from `.env.example`, and creates the two data volumes (`pocr-postgres-data`, `pocr-redis-data`). It ends with a list of what is still left to configure. It is safe to re-run at any time: it never replaces a secret or volume that exists.

Edit `/srv/pocr/.env`. Each setting is explained in the file:

| Variable             | What to put there                                                                                                                                                                                       |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `POCR_HOST`          | `<your-host>`.                                                                                                                                                                                          |
| `FHIR_ENV_MAP`       | Each Epic environment's FHIR base URL and whether it is `production` or `nonproduction`, as `url=environment` pairs separated by `;`. A request whose `fhirServer` matches no entry gets an "Unrecognized environment" card and is not recorded. |
| `CDS_ALLOW_FROM`     | Epic Interconnect addresses or CIDR ranges, separated by spaces or commas. Empty refuses every caller.                                                                                                  |
| `POCR_ALLOWED_USERS` | Email addresses of the people who may log in, comma-separated.                                                                                                                                          |
| `OAUTH_*`            | Your identity provider registration. For Google, set `OAUTH_PROVIDER=google` and delete the three URL lines.                                                                                            |

Leave `POCR_IMAGE_REPO`, `POCR_VERSION`, and the two `VOUCH_*` keys as they are; the bundle and `pocr-init` set them.

`FHIR_ENV_MAP` is not an access control (the caller writes `fhirServer` itself); it keeps test traffic out of production data. `CDS_ALLOW_FROM` and your firewall are the access control. Admitting a whole email domain instead of named users is possible (`POCR_ALLOWED_EMAIL_DOMAINS`) but not recommended until S1 is fixed.

### 3. Install the TLS certificate

nginx serves `certs/ssl.pem` (the certificate, then its intermediates) and `certs/ssl.key`. Pick one option; each is a `pocr-init` option that needs `POCR_HOST` set first.

| Option                                                     | Use it for                                   | Needs                            |
| ---------------------------------------------------------- | -------------------------------------------- | -------------------------------- |
| [A. Your institution's CA](#a-your-institutions-ca)        | Production, and any server Epic calls        | Your PKI or network team         |
| [B. Let's Encrypt](#b-lets-encrypt)                        | A demo or test server with a public DNS name | Inbound TCP 80 from the internet |
| [C. Self-signed](#c-self-signed-testing-only)              | A throwaway install that only you use        | Nothing                          |

#### A. Your institution's CA

```sh
sudo ./pocr-init --csr          # makes certs/ssl.key and prints certs/pocr.csr
```

Send the CSR through your normal certificate request process; the private key never leaves the server. When the certificate comes back, put it in `certs/ssl.pem` with its intermediates after it (not the root), and re-run `pocr-init`, which checks that the certificate matches the key and `POCR_HOST`:

```sh
cat your-host.crt intermediate.crt | sudo tee certs/ssl.pem >/dev/null
sudo ./pocr-init
```

If your team issues the key as well, put it in `certs/ssl.key` instead of using `--csr`.

#### B. Let's Encrypt

Let's Encrypt certificates are free and trusted by browsers; check with your Epic team before relying on one for Epic traffic. Let's Encrypt connects to `<your-host>` on **port 80 from the internet** when it issues the certificate and at each automatic renewal (certificates last 90 days), so before you start:

- `dig +short <your-host>` from outside your network prints this server's public address. If your DNS provider proxies traffic (Cloudflare's orange cloud, for example), turn proxying off for this record: POCR must see callers' real addresses.
- TCP 80 is open to everyone. Nothing listens on it except certbot while it runs.

```sh
sudo apt-get install -y certbot
sudo ./pocr-init --letsencrypt you@example.org      # a real address, for expiry notices
```

This gets the certificate, copies it into `certs/`, and installs a certbot hook that copies in each renewal and reloads nginx. Renewal itself is run by the certbot package's timer.

#### C. Self-signed (testing only)

```sh
sudo ./pocr-init --self-signed
```

This makes a 30-day certificate. Browsers warn about it, Epic refuses it, and the [CDS Hooks sandbox](#try-it-with-the-public-cds-hooks-sandbox) usually cannot use it.

**Before moving on,** run `sudo ./pocr-init` once more: under "Still to do" it should print `Nothing`.

### 4. Start

```sh
sudo docker compose pull
sudo docker compose up -d
sudo docker compose ps
```

All six services should be `Up` within about a minute, with `app`, `engine`, `postgres`, and `redis` `(healthy)`. The engine creates and migrates its database schema on start.

### 5. Verify

From a machine **not** in `CDS_ALLOW_FROM`:

```sh
H=https://<your-host>
curl -s -o /dev/null -w '%{http_code} %{redirect_url}\n' $H/                 # expect 302 to /login
curl -s -o /dev/null -w '%{http_code}\n' $H/api/v1/studies                   # expect 401
curl -s -o /dev/null -w '%{http_code}\n' -H 'X-Vouch-User: a@example.org' $H/api/v1/studies   # expect 401
curl -s -o /dev/null -w '%{http_code}\n' $H/cds-services                     # expect 403
```

From an address **in** `CDS_ALLOW_FROM`, with `fhirServer` set to a `nonproduction` URL from your `FHIR_ENV_MAP`:

```sh
curl -s $H/cds-services                                                      # expect a service with id "randomization"
curl -s -H 'content-type: application/json' $H/cds-services/randomization -d '{
  "hook": "patient-view", "hookInstance": "00000000-0000-0000-0000-000000000001",
  "fhirServer": "https://<epic-tst-fhir-base>/api/FHIR/R4",
  "context": {"patientId": "test-patient", "userId": "Practitioner/test"},
  "prefetch": {"patient": {"resourceType": "Patient", "id": "test-patient"}}
}'                                                                           # expect a "No locator found" card
```

"Unrecognized environment" instead means `fhirServer` matches nothing in `FHIR_ENV_MAP`. For a full Epic-shaped randomization, see [epic-integration.md](epic-integration.md).

Then check that nginx sees real client addresses: `sudo docker compose logs nginx | tail` should show the addresses you called from. If every request shows the same private address, the allow-list cannot tell callers apart; enforce `CDS_ALLOW_FROM` in your firewall.

Finally, open `https://<your-host>/` in a browser, log in, and create a draft study.

### 6. Set up backups

Postgres holds every assignment and the audit trail: the record that proves allocation integrity. Back it up nightly and ship the dumps off the host:

```sh
echo '15 2 * * * root /srv/pocr/pocr-backup' | sudo tee /etc/cron.d/pocr-backup
sudo ./pocr-backup                                  # run once now
```

`pocr-backup` writes `backups/pocr-<date>-<time>.dump` and deletes dumps older than 30 days (set `KEEP_DAYS` to change that). Point your backup system at `/srv/pocr/backups`; the dumps contain PHI.

Run the restore drill once now, and then on a schedule. It restores into a throwaway container and leaves production alone:

```sh
sudo ./pocr-backup --test-restore backups/pocr-<date>-<time>.dump
```

It prints the study and randomization counts in the restored copy next to the live ones.

The install is done. Next: [connect Epic](epic-integration.md) and [set up a study](study-setup.md).

---

## Running POCR

### Commands

All from `/srv/pocr`, with `sudo` unless your user is in the `docker` group.

| Task                         | Command                                                                                                        |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Status                       | `docker compose ps`                                                                                            |
| Engine logs                  | `docker compose logs -f engine`                                                                                |
| Apply a change to `.env`     | `docker compose up -d` (recreates what changed; `docker compose restart` does not re-read `.env`)              |
| Check the configuration      | `./pocr-init`                                                                                                  |
| Back up now                  | `./pocr-backup`                                                                                                |
| Test a backup                | `./pocr-backup --test-restore backups/<file>.dump`                                                             |
| Upgrade                      | `./pocr-upgrade <version>`                                                                                     |
| Readiness (DB + Redis)       | `docker compose exec engine wget -qO- http://127.0.0.1:3000/health/ready`                                      |
| Stop everything (data kept)  | `docker compose down`                                                                                          |

Never run `docker compose down -v` out of habit. The data volumes here are external, so Compose will not delete them, but the habit deletes volumes on other systems.

### Upgrading

Read the release notes, then:

```sh
sudo ./pocr-upgrade 1.2.0
```

It downloads and checks the new bundle, backs up the database, replaces `compose.yaml`, the scripts, and `.env.example`, sets `POCR_VERSION` in `.env`, runs `pocr-init`, and restarts. It lists any settings the new `.env.example` has that your `.env` lacks. Your `.env`, `secrets/`, `certs/`, and `backups/` are not touched. Schema migrations run when the new engine starts.

To roll back, set `POCR_VERSION` back in `.env` and run `docker compose up -d`. If the new version migrated the schema, restore the pre-upgrade dump as well.

Third-party images (`postgres:18-alpine`, Vouch) get security patches under the same tag: `docker compose pull && docker compose up -d` picks them up.

A **Postgres major version** change (18 to 19) changes the on-disk format, so the new image will not start on the old volume. Treat it as planned maintenance: take a final dump, stop the stack, create a fresh volume, start the new version on it, restore the dump, verify, and only then remove the old volume.

### Backups and Redis

Redis needs no backup. It keeps an append-only file on its volume and survives restarts. If its data is lost, no assignment is lost (they are in Postgres, and repeat requests return the original assignment). What is lost is each study's _in-progress_ permuted block: the next request starts a fresh block, so arm counts can drift by up to one block's worth of imbalance per stratum.

To restore production from a dump, stop the engine (`docker compose stop engine`), then `docker compose exec -T postgres pg_restore -U pocr -d pocr --clean --if-exists < backups/<file>.dump`, then `docker compose up -d`.

### Certificate renewal

- **Let's Encrypt:** automatic. `sudo certbot renew --dry-run` checks that renewal works.
- **Your institution's CA:** note the expiry date `pocr-init` prints. To renew, copy the new certificate over `certs/ssl.pem` (with `cp` or `tee`, not `mv`, so the containers see the change), run `sudo ./pocr-init` to check it, and reload nginx: `sudo docker compose exec nginx nginx -s reload`.

---

## Try it with the public CDS Hooks sandbox

The [CDS Hooks sandbox](https://sandbox.cds-hooks.org) is a web page that plays the part of an EHR: it shows a synthetic patient and calls your CDS services. It is the quickest way to see POCR return a card without any Epic work. To try it on a laptop instead, see [external-dev-guide.md](external-dev-guide.md#trying-the-cds-hooks-sandbox-locally).

The sandbox runs **in the browser**: the browser makes the calls to POCR. The server must be reachable from that browser with a certificate it trusts (option A or B).

Use it only on a server with synthetic data and no Epic connection. These settings publish study names and let anyone on the allow-list create randomizations ([security-concerns.md](security-concerns.md) S2–S4).

**Set up the server.** Add to `.env`, then run `sudo docker compose up -d`:

```sh
FHIR_ENV_MAP=https://launch.smarthealthit.org/=nonproduction   # where the sandbox's patients come from
CDS_ALLOW_FROM=<browser-ips>      # public IP of each computer that will run the sandbox, or `all`
CDS_CORS_ORIGINS=https://sandbox.cds-hooks.org                  # lets the sandbox page read POCR's responses
CDS_DISCOVERY_LIST_STUDIES=true   # lists each open study as its own service
```

`curl -s https://checkip.amazonaws.com` on a computer shows its public IP. The sandbox cannot send the Epic locator POCR normally uses to find the study, so it calls each study's own endpoint, `/cds-services/<study-id>`, which `CDS_DISCOVERY_LIST_STUDIES` makes visible.

**Run it.**

1. Log in to `https://<your-host>/` and create a study (draft is fine), e.g. two arms, block randomization, key "patient ID".
2. In the same browser, open `https://sandbox.cds-hooks.org/?serviceDiscoveryURL=https://<your-host>/cds-services`.
3. On the Patient View tab you should see a "Randomized: <arm>" card from POCR. Reloading shows the same assignment; _Change Patient_ gives a new one. In the Change Patient dialog, click _Save_ once and close the dialog with its ×: a second _Save_ clears the cards.
4. The sandbox also calls the Epic-style `randomization` service and shows its "No locator found" card. That is expected; turn it off under gear → _Configure CDS Services_.
5. In the console, set the data environment selector in the header to **Non-Production** and open the study's **Monitoring** tab. Each sandbox patient is listed under Recent Randomizations. **Purge Non-Production Data** on the same tab clears them.

| If you see                                      | Cause                                                                                                                                                       |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The sandbox cannot load the services            | The browser doesn't trust the certificate or can't reach the server. Open `https://<your-host>/cds-services` in the same browser to see which.              |
| `403` on `/cds-services`                        | The browser's public address is not in `CDS_ALLOW_FROM` (it changes when you switch networks). `sudo docker compose exec nginx cat /etc/nginx/snippets/cds-access.conf` shows the list in use. |
| A CORS error in the browser console             | `CDS_CORS_ORIGINS` is not set, or `docker compose up -d` was not run after setting it.                                                                      |
| Only the "No locator found" card                | `CDS_DISCOVERY_LIST_STUDIES` is not `true`, or the study is closed.                                                                                         |
| "Unrecognized environment" card                 | `FHIR_ENV_MAP` does not include `https://launch.smarthealthit.org/`.                                                                                        |

**Afterwards,** remove `CDS_CORS_ORIGINS` and `CDS_DISCOVERY_LIST_STUDIES`, restore `FHIR_ENV_MAP` and `CDS_ALLOW_FROM` to your Epic values, run `sudo docker compose up -d`, and purge the sandbox data from each study.

---

## Verifying the images, or building them yourself

**Signatures.** Each image is signed by the project's release workflow with [Sigstore cosign](https://docs.sigstore.dev/), and carries an SBOM and build provenance for your vulnerability scanning. With cosign installed and `V` set as in step 1:

```sh
for i in engine app redis nginx; do
  cosign verify ghcr.io/nitidbit/pocr-$i:$V \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    --certificate-identity-regexp '^https://github.com/nitidbit/pocr/\.github/workflows/release\.yml@refs/tags/v'
done
docker buildx imagetools inspect ghcr.io/nitidbit/pocr-engine:$V --format '{{json .SBOM}}'
```

**Building from source,** for sites whose policy requires it. You need `git` and Docker Buildx; the engine build compiles Rust and takes several minutes.

```sh
git clone --branch v$V https://github.com/nitidbit/pocr.git pocr-src
cd pocr-src
RELEASE_REPO=pocr-local TAG=$V docker buildx bake -f docker-bake.hcl release --load
```

That produces `pocr-local/pocr-{engine,app,redis,nginx}:$V` in the local image store. Install as above (the bundle files are in the checkout's `deploy/` directory), with `POCR_IMAGE_REPO=pocr-local` in `.env`, and start with `docker compose up -d --pull never` instead of pulling.

---

## Troubleshooting

| Symptom                                                                                   | Cause and fix                                                                                                                                                                                                                 |
| ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `docker compose` fails: `set POCR_HOST in .env` (or another variable)                     | `.env` missing or incomplete (step 2). Run compose from `/srv/pocr`.                                                                                                                                                          |
| `docker compose` fails: `run pocr-init to set VOUCH_JWT_SECRET`                           | Run `sudo ./pocr-init`.                                                                                                                                                                                                       |
| `docker compose` fails: `secret ... file ... not found` or a certificate bind mount fails | `secrets/` or `certs/` missing. Run `sudo ./pocr-init` (step 2) and install the certificate (step 3).                                                                                                                         |
| `docker compose pull` fails: `UNAUTHORIZED: authentication required`                      | The project's images are public; a private mirror needs `sudo docker login <registry>` first (and `cosign login <registry>` for verification).                                                                                |
| `pocr-init` says `secrets/ is partly filled`                                              | Some secret files were deleted. See its message: Postgres keeps the password it was first started with.                                                                                                                       |
| `pocr-init` warns the certificate is not valid for `POCR_HOST`, or the key doesn't match  | The certificate was issued for another name, or `certs/ssl.key` is not its key. With `--csr`, don't re-run `--csr` after deleting `certs/ssl.key`: the old CSR's certificate will not match the new key.                     |
| `pocr-init --letsencrypt`: certbot fails                                                  | `<your-host>` does not resolve to this server from the internet, or TCP 80 is not open to everyone. certbot's message says which.                                                                                             |
| `vouch` restarting                                                                        | `docker compose logs vouch` names the setting. Common: an `OAUTH_*` value missing, or `callback_url ... must be within a configured domains` (`POCR_HOST` mismatch).                                                          |
| Login loops back to the login page, or the IdP says "redirect URI mismatch"               | The IdP's registered redirect URI must be exactly `https://<POCR_HOST>/auth`.                                                                                                                                                 |
| Logged in but "not authorized" (Vouch 403 page)                                           | Your email isn't in `POCR_ALLOWED_USERS`. Change it, then `docker compose up -d`.                                                                                                                                             |
| nginx: `host not found in upstream "vouch:9090"`                                          | Vouch wasn't up yet; nginx restarts and recovers. If it persists, fix Vouch first.                                                                                                                                            |
| Engine exits: `password authentication failed`                                            | Postgres only reads its password when the volume is first initialized. If `secrets/` was regenerated after that, change the password inside Postgres to match `secrets/postgres_password` (`ALTER USER pocr PASSWORD '...'`). |
| Engine exits: `failed to read DATABASE_URL_FILE ... permission denied`                    | Secret file not readable by uid 1001. Re-run `sudo ./pocr-init` as root, which fixes ownership.                                                                                                                               |
| CDS call returns "Unrecognized environment"                                               | `fhirServer` in the request doesn't start with a `FHIR_ENV_MAP` prefix. The engine log shows the URL received.                                                                                                                |
| CDS call returns `403`                                                                    | The caller's address isn't in `CDS_ALLOW_FROM`. Change it, then `docker compose up -d`.                                                                                                                                       |
| Epic shows no card for an assigned patient                                                | Expected for every arm except the one assigned: POCR answers `412` when the request's locator belongs to a different arm. See [epic-integration.md](epic-integration.md).                                                     |
