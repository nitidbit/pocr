# POCR for Installing Sites — Overview and Readiness Checklist

**Audience:** the health-system IT lead deciding whether and how to run POCR, and the people they will pull in (server admin, Epic analyst, security reviewer, study team).
**Status:** written 2026-10-14 against the code in this repository. Steps that depend on Epic were reconstructed from UCSF's notes and have not been re-verified outside UCSF; they are marked as such.

---

## What POCR is

POCR (Point of Care Randomization) assigns patients (or encounters, or providers) to study arms at the moment a clinician is in the chart. It was built at UCSF for pragmatic trials embedded in clinical care and replaces an older engine written in Epic's own Caché code.

The workflow:

1. **Define a study** in POCR's web console: its arms and their ratio (e.g. Control 1 : Drug A 2 : Drug B 2), simple or permuted-block randomization, the unit being randomized (patient, encounter, provider), and any stratification factors (e.g. sex).
2. **Configure the EHR** to call POCR at the right point in the workflow, sending the data POCR needs (patient, encounter, any stratification fields).
3. **The EHR calls POCR** when that point is reached. POCR assigns an arm, records it, and returns a card the EHR shows to the clinician. Asking again for the same unit returns the same assignment.
4. **Export** assignments and audit history from the console for analysis.

The EHR talks to POCR using **CDS Hooks**, an open HL7 standard for "the EHR asks an outside service for advice and shows the answer as a card". Epic supports it; so do other EHRs and the public CDS Hooks sandbox.

## How a site runs it

**Each site runs its own, fully independent POCR server.** There is no central UCSF service, no data sent to UCSF, and no license. (Older UCSF material describes a "satellite / mothership" model where sites registered with a UCSF server; that is not how this version works.)

```
            ┌──────────── your network ────────────────────────────────────┐
            │                                                              │
 Epic  ─────┼─ HTTPS POST /cds-services/randomization ─┐                   │
 (Interconnect)                                       ▼                   │
            │                                   ┌──────────┐              │
 Study staff┼─ HTTPS (browser) ────────────────▶│  nginx   │ TLS, routing │
 (console)  │                                   └────┬─────┘              │
            │              login check (OAuth/OIDC)  │                    │
            │                 ┌───────┐◀─────────────┤                    │
            │                 │ Vouch │              │                    │
            │                 └───────┘        ┌─────┴─────┐  ┌────────┐  │
            │                                  │  engine   │  │  app   │  │
            │                                  │  (Rust)   │  │ (React)│  │
            │                                  └──┬─────┬──┘  └────────┘  │
            │                          ┌──────────┘     └──────┐          │
            │                    ┌─────▼──────┐          ┌─────▼─┐        │
            │                    │ PostgreSQL │          │ Redis │        │
            │                    └────────────┘          └───────┘        │
            └──────────────────────────────────────────────────────────────┘
```

Everything runs as Docker containers on one Linux host, from the `compose.yaml` in this repository:

| Container | Role |
|---|---|
| **nginx** | The only thing exposed to the network. Terminates TLS, sends `/cds-services` straight to the engine, and requires a login for everything else. |
| **vouch** | Handles login for the web console against your identity provider (Google, Okta, Azure AD/Entra, any OIDC provider). |
| **engine** | The randomization service and the console's API. |
| **app** | The web console (static files). |
| **postgres** | The system of record: studies, every assignment, the audit log. Back this up. |
| **redis** | Speeds things up and holds the in-progress permuted block for each study/stratum. Losing it is recoverable (see the install guide). |

POCR **never calls back into Epic**. Epic sends everything POCR needs inside the request (CDS Hooks "prefetch"), so POCR needs no Epic credentials, no FHIR client registration for outbound calls, and no outbound network access to Epic.

## What POCR stores

Each assignment row holds the EHR patient ID, encounter CSN, the Epic user ID of the clinician, the study, arm, and time. That is PHI, so the server, database volume, and backups fall under your normal PHI handling. See [security-concerns.md](security-concerns.md) S7.

## Known limitations to plan around

Read [security-concerns.md](security-concerns.md) before go-live. The two that affect deployment design:

- **The CDS Hooks endpoint does not verify Epic's signed token yet** (S2). Until it does, restrict `/cds-services` at the network level to your Epic Interconnect servers.
- **Console users are not restricted per study yet** (S1). Anyone who can log in can change any study. Limit login to a short named list of people.

Others: no blinding (the card names the arm), and the frontend is the only supported way to configure studies. [feature-status.md](feature-status.md) lists what is built and what is not.

---

## Readiness checklist

Work through this before scheduling an install. Each line names who usually owns it.

### People

| Role | What they do for POCR | Needed for |
|---|---|---|
| **Server / platform admin** | Provides a Linux VM, installs Docker, runs the install guide, owns backups and patching. | Install |
| **Network / firewall admin** | DNS name, TLS certificate, firewall rules so Epic can reach the server and only Epic can reach `/cds-services`. | Install |
| **Identity admin** | Registers POCR as an OAuth/OIDC application so study staff can log in to the console. | Install |
| **Epic Interconnect / client admin** | Creates the client ID and external endpoint record Epic uses to call POCR, and the JWT settings for that call. | Epic integration |
| **Epic analyst (CDS / OPA)** | Builds the criteria and BestPractice Advisory (or rule) that calls POCR at the right moment and shows the result. | Each study |
| **Study team / PI** | Defines arms, ratios, strata in the console; signs off that test randomizations behave as designed. | Each study |
| **Security / privacy reviewer** | Reviews this document set, security-concerns.md, and the PHI stored. | Before go-live |
| **Report writer** (optional) | Pulls assignment data for analysis, or wires up the optional Caboodle demographics sync. | Analysis |

### Infrastructure decisions

- [ ] **Host:** a Linux x86-64 VM with Docker Engine and the Compose plugin. 2 vCPU / 4 GB RAM / 40 GB disk is plenty for the expected load. Put it close (network-wise) to your Epic Interconnect servers; UCSF measured ~300 ms for a full request with a co-located server.
- [ ] **DNS name** for the server, e.g. `pocr.your-health-system.org`.
- [ ] **TLS certificate** for that name from a CA Epic trusts (your internal CA is usually fine for an internal-only server).
- [ ] **Reachability:** is the console reachable only on your internal network/VPN (recommended), or from the internet? `/cds-services` only needs to be reachable from Epic Interconnect.
- [ ] **Identity provider** for console login, and the list of people allowed in.
- [ ] **Backups:** where nightly database dumps go (off the host), and who tests restores.
- [ ] **Environments:** which Epic environments will call this server. Each Epic environment's FHIR base URL is mapped to either `production` or `nonproduction` in POCR (`FHIR_ENV_MAP`). Many sites run one POCR server for all Epic environments; others run a separate test server.

### Epic prerequisites (for the Epic analyst; unverified outside UCSF)

- [ ] Your Epic version supports CDS Hooks from OPA / BestPractice Advisory criteria.
- [ ] Someone who can create an **external endpoint record** (E0A/E0E) and a client ID with the **CDS Hooks** outgoing API scope. Adam Wright noted that getting these records created can be slow; start early.
- [ ] Your Interconnect servers can make outbound HTTPS calls to the POCR server.
- [ ] For each study: one OPA criteria record per arm (POCR identifies the study and arm by the criteria ID Epic sends).

### Before the first live study

- [ ] Install done and verified ([install.md](install.md)).
- [ ] Optional: the public CDS Hooks sandbox has been used to see POCR working end to end without Epic ([install.md](install.md#try-it-with-the-public-cds-hooks-sandbox)).
- [ ] Test randomizations from a non-production Epic environment land as `nonproduction` and can be purged.
- [ ] Backup job running and one restore tested.
- [ ] Security review of S1 and S2 mitigations completed.

## Document map

| Document | For |
|---|---|
| **overview.md** (this file) | Deciding, staffing, and planning |
| [install.md](install.md) | Server admin: installing, configuring, upgrading, backing up |
| [security-concerns.md](security-concerns.md) | Security reviewer: known issues and mitigations |
| [feature-status.md](feature-status.md) | What is built, what is partial, what is planned |
| [production-persistence.md](production-persistence.md) | Database durability details behind the install guide's backup section |
| [external-dev-guide.md](external-dev-guide.md) | Developers working on POCR without Epic access |
| *Planned:* Epic integration guide, study setup guide, operations/troubleshooting | |
