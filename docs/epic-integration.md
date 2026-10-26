# Connecting Epic to POCR

_Last updated: 2026-10-26_

This guide is for the Epic analysts and Interconnect/client admins who connect an Epic environment to an installed POCR server ([install.md](install.md)). The console side of each study is in [study-setup.md](study-setup.md).

> **Unverified on the Epic side.** The Epic build steps below come from working builds at other sites, not from a build the maintainers could test; record and screen names vary by Epic version. POCR's side (what it reads from the request and what it returns) is checked against the code, and the [example requests](#testing-without-epic) were run against POCR on 2026-10-26.

---

## How it works

- Each study arm has its own **OPA criteria record** with a Web Service criterion that calls POCR through an **external endpoint record**. A BestPractice Advisory (or a rule) uses that criteria record.
- Epic sends the criteria record's ID in the request as `extension["com.epic.cdshooks.request.criteria-id"]`. In POCR, that ID is the arm's **locator**.
- POCR finds the study from the locator and assigns the patient to an arm. The first call for a patient makes the assignment; every later call, from any arm's criteria, returns the same one.
- POCR returns a card **only to the criteria record of the assigned arm**. Calls from the other arms' criteria get HTTP 412, so their criteria do not match and their advisories do not show.
- POCR reads everything it needs from the request (`context` and `prefetch`). It never calls back into Epic.

So for a two-arm study, Epic calls POCR twice when the advisories are evaluated, and exactly one advisory shows.

---

## Who does what

| Step | Who | How often |
|---|---|---|
| [1. Client ID](#1-client-id) | Epic client / Vendor Services admin | Once per site |
| [2. External endpoint record](#2-external-endpoint-record) | Interconnect / client systems | Once per Epic environment |
| [3. POCR settings](#3-pocr-settings) | POCR server admin | Once per Epic environment |
| [4. Criteria records and advisories](#4-criteria-records-and-advisories) | Epic CDS / OPA analyst | Each study |
| [5. Locators](#5-locators) | Study coordinator, in the POCR console | Each study |
| [6. Test](#6-test-in-non-production) | Analyst and coordinator | Each study |

---

## Once per site

### 1. Client ID

Create an application in Epic's Vendor Services (fhir.epic.com) for your organization:

- **Outgoing API:** the **CDS Hooks** framework scope. This is required.
- **Incoming APIs:** none are needed; POCR never calls Epic.
- **Endpoint URI:** `https://<POCR_HOST>/cds-services/randomization`. Add one per POCR server (for example a test server and a production server).

The application has a production and a non-production client ID. Have it approved and synced to your environments as your site requires.

### 2. External endpoint record

Create one external endpoint record (E0A/E0E) in each Epic environment that will call POCR:

| Field | Value |
|---|---|
| Endpoint type | REST |
| Client ID | From step 1 (non-production ID in non-production environments) |
| Address | `https://<POCR_HOST>/cds-services/randomization` |
| HTTP method | POST |
| Content type | `application/json` |
| Authentication | JWT |
| Encryption | SSL |
| Mode | SYNC |
| Server timeout | Your choice. The reference build used 5 seconds. |

JWT claims:

| Claim | Value |
|---|---|
| `aud` | `https://<POCR_HOST>/cds-services/randomization` |
| `iss` | This environment's FHIR R4 base URL |
| `sub` | The client ID |

POCR does not verify this JWT yet ([security-concerns.md](security-concerns.md) S2); fill it in so it is ready when it does.

Epic must trust the POCR server's TLS certificate, and the Interconnect servers must be able to reach `POCR_HOST` on port 443.

### 3. POCR settings

Give the POCR server admin, for each Epic environment:

- **The FHIR base URL Epic sends as `fhirServer`**, and whether the environment is production or non-production. It goes in `FHIR_ENV_MAP` in `.env`, for example `https://epic.example.org/interconnect-tst/api/FHIR/R4=nonproduction`. POCR matches on prefix. A request whose `fhirServer` matches nothing is refused. If you are unsure of the exact URL, fire one advisory and look for `unrecognized fhirServer` in the POCR logs ([Troubleshooting](#troubleshooting)), which prints the URL Epic sent.
- **The source addresses of your Interconnect servers.** They go in `CDS_ALLOW_FROM`. Anything else gets HTTP 403 from POCR's nginx.

Production and non-production requests are kept apart: a non-production environment can never write production assignments ([study-setup.md](study-setup.md#3-test-in-non-production)).

---

## Each study

### 4. Criteria records and advisories

First create the study in the POCR console as a draft ([study-setup.md](study-setup.md)), so you know its arms, unit of randomization, and strata.

Then, **for each arm:**

1. Create an OPA criteria record with a **Web Service** criterion that uses the external endpoint record from step 2.
2. Add these prefetch templates. The keys must be exactly these lowercase names:

   | Key | Template |
   |---|---|
   | `patient` | `Patient/{{context.patientId}}` |
   | `encounter` | `Encounter/{{context.encounterId}}` |
   | `user` | `{{context.userId}}` |

3. Build the BestPractice Advisory (or rule) that uses this criteria record. Put the arm-specific instruction for the clinician in the advisory; POCR's card names the arm but carries no study content.
4. Note the criteria record's ID. That is the arm's locator.

Triggering: POCR does not check which hook or trigger fired, so any place that evaluates the criteria (an advisory trigger, or a rule that evaluates the OPA) can randomize. The context POCR needs must be present at that moment. An encounter-level study fired with no encounter in context fails.

What POCR reads from the request:

| Needed for | Read from | If missing |
|---|---|---|
| Study and arm | `extension["com.epic.cdshooks.request.criteria-id"]` | "No locator found" card |
| Environment | `fhirServer` | "Unrecognized environment" card |
| Unit of randomization | `context.patientId`, `context.encounterId`, or `context.userId`, according to the study's keys | Error (500), no assignment |
| Strata | The prefetched resource and path set in the study (for example `patient` → `gender`) | Error (500), no assignment |
| Recorded identifiers | `patient`: identifier with `type.text` = `EXTERNAL`; `encounter`: identifier whose `system` ends in `.7.3.698084.8` (CSN); `user`: identifier with `type.text` = `EXTERNAL` | Stored empty; the assignment still succeeds |

The recorded identifiers are what tie an assignment back to a patient, encounter, and clinician in reports. Check after testing that they are filled in ([step 6](#6-test-in-non-production)).

### 5. Locators

In the POCR console, enter each arm's criteria record ID as its locator ([study-setup.md](study-setup.md#2-map-each-arm-to-epic)). Every arm needs one; a patient assigned to an arm without a locator is randomized but no advisory shows.

### 6. Test in non-production

1. From a non-production Epic environment, open the chart (or do whatever fires the advisories) for several test patients.
2. Each patient should see **exactly one** arm's advisory, and the same one every time.
3. In the POCR console, switch to **Non-Production** and check the study's **Monitoring** tab.
4. Ask the POCR admin to confirm the recorded identifiers (`epicPatientId`, `epicCsn`, `epicUserId`) are filled in, for example from `GET /api/v1/studies/<study-id>/randomizations?environment=nonproduction`.
5. Purge the non-production data when done.

Release the production advisories only after the study is **Active** in POCR, and retire them before closing it (see the next section for why).

---

## What POCR returns

| Situation | HTTP | Response |
|---|---|---|
| Assigned arm's locator is this criteria ID | 200 | Assignment card (below) |
| Assigned arm has a different locator, or none | 412 | No card |
| No study has this locator | 412 | No card |
| `fhirServer` not in `FHIR_ENV_MAP` | 200 | Info card "Unrecognized environment" |
| No criteria ID in the request | 200 | Info card "No locator found" |
| Production request and study not Active, or study Closed | 200 | Info card "Randomization unavailable" |
| A unit key or stratum value is missing | 500 | No card |

The assignment card:

```json
{
  "uuid": "01a179d9-f1c0-7742-a4d2-ca970e6adaba",
  "summary": "Randomized: Intervention",
  "detail": "Study: Epic guide test\nArm: Intervention (ratio 1)\nAssignment ID: 01a179d9-f1c0-7742-a4d2-ca970e6adaba",
  "indicator": "warning",
  "source": { "label": "POCR", "url": "https://randomization.ucsf.edu" }
}
```

The `source.url` is fixed in the current release ([epic-dependency.md](epic-dependency.md#5-smaller-items)).

**Check the info cards in your build.** They come back with HTTP 200 on every arm's call. If your criteria match whenever a card is returned, every arm's advisory would show the info card, for example while a production advisory is live and the study is still Draft. Test one of these cases (a study left in Draft, called from a production-mapped environment) before go-live.

---

## Testing without Epic

These requests have the shape Epic sends: the criteria-ID extension, a CSN with the Epic OID, and `EXTERNAL` patient and user identifiers. Use them to check a POCR server before Epic is connected, or to compare against what Epic sends.

**Setup.** Either:

- **Local dev stack:** `docker compose -f compose.dev.yaml up -d engine`. Its `FHIR_ENV_MAP` already maps `https://fake-epic.localhost/` to non-production, and `/cds-services` is open. Use `POCR=http://localhost:3000`.
- **Installed server:** add `https://fake-epic.localhost/=nonproduction` to `FHIR_ENV_MAP` and your address to `CDS_ALLOW_FROM`, then `docker compose up -d`. Use `POCR=https://<POCR_HOST>`. Remove both entries afterwards.

Then create a study in the console with two arms, unit **Patient ID**, and stratum **Sex / Gender**, and set the arms' locators to `11131` and `11132`.

Paste this function into your shell:

```sh
POCR=http://localhost:3000

# epic_request CRITERIA_ID PATIENT_ID [FHIR_SERVER]
epic_request() {
  curl -sS -w '\nHTTP %{http_code}\n' -X POST "$POCR/cds-services/randomization" \
    -H 'Content-Type: application/json' -d @- <<JSON
{
  "hook": "patient-view",
  "hookInstance": "d1577c69-dfbe-44ad-ba6d-3e05e953b2ea",
  "fhirServer": "${3:-https://fake-epic.localhost/api/FHIR/R4}",
  "context": {
    "patientId": "$2",
    "encounterId": "eCSN830100123",
    "userId": "Practitioner/eUSER1234"
  },
  "extension": {
    "com.epic.cdshooks.request.criteria-id": "$1"
  },
  "prefetch": {
    "patient": {
      "resourceType": "Patient",
      "id": "$2",
      "gender": "female",
      "identifier": [
        { "type": { "text": "EXTERNAL" },
          "system": "urn:oid:1.2.840.114350.1.13.0.1.7.2.698084",
          "value": "Z1001" }
      ]
    },
    "encounter": {
      "resourceType": "Encounter",
      "id": "eCSN830100123",
      "identifier": [
        { "system": "urn:oid:1.2.840.114350.1.13.0.1.7.3.698084.8",
          "value": "830100123" }
      ]
    },
    "user": {
      "resourceType": "Practitioner",
      "id": "eUSER1234",
      "identifier": [
        { "type": { "text": "EXTERNAL" },
          "system": "urn:oid:1.2.840.114350.1.13.0.1.7.2.697780",
          "value": "1234" }
      ]
    }
  }
}
JSON
}
```

**1. Both arms' criteria fire for one patient.** Exactly one returns a card; which one is random.

```sh
epic_request 11131 eP1
epic_request 11132 eP1
```

```
{"detail":"precondition failed: Assigned arm is for a different locator","error":"precondition_failed"}
HTTP 412
{"cards":[{"uuid":"01a179d9-...","summary":"Randomized: Intervention","detail":"Study: Epic guide test\nArm: Intervention (ratio 1)\nAssignment ID: 01a179d9-...","indicator":"warning","source":{"label":"POCR","url":"https://randomization.ucsf.edu"}}]}
HTTP 200
```

Run them again: the same arm answers, with the same assignment ID. The assignment appears under **Non-Production** in the console with patient ID `Z1001`, CSN `830100123`, and user ID `1234`.

**2. An unknown criteria ID** (no arm has it as a locator):

```sh
epic_request 99999 eP2
```

```
{"detail":"precondition failed: No study configured for this locator","error":"precondition_failed"}
HTTP 412
```

**3. An Epic environment POCR does not know:**

```sh
epic_request 11131 eP2 https://other.example.org/api/FHIR/R4
```

```
{"cards":[{"summary":"Unrecognized environment","detail":"This EHR instance is not configured for randomization.","indicator":"info","source":{"label":"POCR"}}]}
HTTP 200
```

---

## Troubleshooting

Watch POCR's log while firing an advisory. On the POCR server, in the install directory:

```sh
docker compose logs -f engine
```

| Epic shows / log says | Cause |
|---|---|
| Web service call failed, and no POCR log line | The request never reached POCR: endpoint address, TLS trust, firewall, or `CDS_ALLOW_FROM` (nginx returns 403; see `docker compose logs nginx`). |
| `rejecting randomization from unrecognized fhirServer` | Add the URL printed in that line to `FHIR_ENV_MAP`. |
| `No study found for locator` | The criteria ID is not entered as any arm's locator, or a wrong ID was entered. |
| `Assigned arm is for a different locator — no card` | Normal: this patient is in another arm. |
| `missing required field` or `not found` (HTTP 500) | The study's unit key is not in `context`, or a stratum's value is not in the prefetched resource. Check the prefetch templates and the trigger's context. |
| Randomization works but identifiers are empty | Prefetch keys are not exactly `patient` / `encounter` / `user`, or the identifiers lack the `EXTERNAL` type or CSN OID. |
