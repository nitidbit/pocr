# Setting Up a Study

_Last updated: 2026-10-26_

This guide covers setting up a study in the POCR console, testing it, and running it. It assumes POCR is installed ([install.md](install.md)) and the EHR side is being built in parallel ([epic-integration.md](epic-integration.md)).

The order is: **create (Draft) → map arms to Epic → test in non-production → activate → monitor → close.**

---

## Before you start

Decide these up front. They are fixed once the study is activated (see [What you can change later](#what-you-can-change-later)), and the console has no edit screen for them even while the study is a draft. To change them you delete the draft and create it again.

| Decision | Options |
| --- | --- |
| Arms | Two or more, each with an integer allocation ratio (1:1, 2:1, …) |
| Strategy | **Simple Randomization** or **Block Randomization** |
| Block size | Block strategy only. A multiple of the sum of the arm ratios, or blank |
| Randomization key | What counts as one subject: Patient ID, Encounter ID, Provider ID, or a combination |
| Stratification factors | Optional. Patient attributes to balance within, e.g. sex |

---

## 1. Create the study

**Studies → Create New Study** opens a five-step wizard. Use **Continue** and **Back** to move between steps.

### Study Information

- **Study Title** (required): the name shown everywhere in the console and on the card the clinician sees.
- **Description**: free text shown on the study's Overview tab.
- **IRB Number** (required), **Short Name**, **IRB Exception**, **Protocol ID**, **Protocol Link**: see the note below.

### Team & Timeline

All optional: PI, coordinator, statistician, **Study Sites**, **Start Date**, **End Date**, **Target Enrollment**.

> **Not saved today.** Only Study Title, Description, and the Randomization Design and Keys & Strata steps are stored. Everything else on these first two steps (including the required IRB Number) is discarded when you click **Create Study**. Keep that information in your study records, not in POCR.

### Randomization Design

- **Strategy**
  - **Simple Randomization**: each subject is assigned independently, weighted by the arm ratios. Arm totals can drift apart, especially in small studies.
  - **Block Randomization**: subjects are assigned from shuffled blocks that contain each arm in proportion to its ratio, so arm totals stay close throughout enrollment. Use this if you add stratification factors; they have no balancing effect under Simple.
- **Block Size** (Block only): must be a multiple of the sum of the arm ratios. With ratios 1:1, valid sizes are 2, 4, 6, …; with 2:1, they are 3, 6, 9, …. Leave it blank to have POCR pick each new block's size at random from 2, 3, or 4 times the ratio sum, which makes the next assignment harder to predict.
- **Treatment Arms**: at least two. Each has a name (shown on the card) and a ratio. **Add Arm** adds more. Arms are listed and stored in the order shown here.

### Keys & Strata

**Randomization Keys** define the subject. A subject who is randomized again in the same study gets the same arm back; no new assignment is made.

| Key | Value used | Typical use |
| --- | --- | --- |
| **Patient ID** | `context.patientId` from the CDS Hooks request | One assignment per patient (most trials) |
| **Encounter ID** | `context.encounterId` | One assignment per visit or admission |
| **Provider ID** | `context.userId` | Cluster by clinician |
| **FHIR Path** | — | **Do not use.** Not implemented; the study will fail to save |

With more than one key, the subject is the combination (for example, Patient ID + Encounter ID gives one assignment per patient per encounter). A request missing any key's value fails.

**Stratification Factors** (optional) balance assignment within subgroups. Each stratum gets its own sequence of blocks.

- **Sex / Gender** adds the patient's FHIR `gender` (label "Sex", context `Patient`, path `gender`).
- **Custom FHIRPath** adds your own. Fill in:
  - **Label**: display name.
  - **Context**: which prefetched resource to read: `Patient`, `Encounter`, or `user`. These are the only resources POCR asks the EHR to prefetch.
  - **FHIRPath**: a dotted path into that resource, e.g. `gender` or `address.0.state`, or a JSON pointer such as `/gender`. This is a simple path lookup, not full FHIRPath expressions.

Every request must carry a value at each stratum's path. If a patient has no value (say, no `gender` on the Patient resource), the request fails and no assignment is made. Pick factors the EHR always populates, and keep the number of factors and categories small: each combination is a separate stratum with its own blocks.

### Review & Confirm

Check the summary (use **Edit** on any section to go back), then click **Create Study**. The study is created in **Draft** and you become its **Owner**.

---

## 2. Map each arm to Epic

Skip this if your EHR calls the per-study endpoint (`/cds-services/{study_id}`), which is the case for non-Epic EHRs and the public CDS Hooks sandbox. See [epic-dependency.md](epic-dependency.md).

On Epic, each arm has its own OPA criteria record and BestPractice Advisory, and each calls POCR. In the study's **Configuration** tab, enter each arm's criteria record ID in the **Locator** column and click the check mark to save it.

- **Every arm needs a locator.** POCR records the assignment first and then shows the card only on the advisory whose locator matches the assigned arm. A patient assigned to an arm with no locator is still randomized, but no clinician ever sees a card.
- Use each locator on one arm in one study only.
- Locators can be set or changed at any time, including after activation. Each change is recorded in the study's **History**.

How the criteria records and advisories are built is covered in [epic-integration.md](epic-integration.md).

---

## 3. Test in non-production

Requests from EHR environments mapped to `nonproduction` in `FHIR_ENV_MAP` ([install.md](install.md)) randomize into a separate record set with its own blocks. They work while the study is **Draft**, **Active**, or **Paused**, so you can test before activating.

1. Fire the advisory (or a test request) from your non-production Epic environment for a few test patients.
2. Switch the **Data environment** selector at the top of the console to **Non-Production**.
3. Open the study's **Monitoring** tab and confirm the assignments appear under the right arms.
4. When finished, click **Purge Non-Production Data** → **Confirm Purge** at the bottom of the Monitoring tab. This deletes this study's non-production assignments and blocks only. Production data is untouched.

---

## 4. Activate

On the study's **Overview** tab, click **Activate Study** → **Confirm**. POCR checks the configuration (at least two arms, positive ratios, valid block size) and then freezes it. Production requests are accepted only while the study is **Active**.

---

## 5. Monitor

The **Monitoring** tab shows the environment selected in the **Data environment** selector:

- **Balance Summary**: assignments per arm, with counts and percentages.
- **Demographic Balance**: production only, and only if the optional Caboodle demographics sync is running ([epic-dependency.md](epic-dependency.md)). Otherwise it reads "No demographics synced yet."
- **Recent Randomizations**: the ten most recent assignments (ID, Arm, Issued At), with the total count.

There is no export from the console yet ([feature-status.md](feature-status.md) #16).

The **History** tab lists every change to the study: status changes, locator assignments, and ratio changes, with who made them and when.

---

## 6. Pause, resume, close

All on the **Overview** tab, each with a confirmation step:

| Action | Available from | Effect |
| --- | --- | --- |
| **Pause Study** | Active | New production requests get a "Randomization unavailable" card. Non-production still works. |
| **Resume Study** | Paused | Production randomization resumes. Blocks continue where they left off. |
| **Close Study** | Draft, Active, Paused | Permanent. All requests, in both environments, get "Randomization unavailable". Data is kept. |
| **Delete Study** | Draft only (the **Danger Zone** section) | Permanently removes the study. A record of the deletion stays in the audit log. |

---

## What you can change later

| Setting | Draft | Active / Paused | Closed |
| --- | --- | --- | --- |
| Arm ratios | ✅ | ✅ (see below) | ✅ but no effect |
| Locators | ✅ | ✅ | ✅ but no effect |
| Arms (add, remove, rename) | Delete and recreate | ❌ | ❌ |
| Strategy, block size | Delete and recreate | ❌ | ❌ |
| Keys, stratification factors | Delete and recreate | ❌ | ❌ |
| Title, description | No UI | No UI | No UI |
| Access (owners and members) | ✅ | ✅ | ✅ |

**Changing ratios on a running study.** In **Configuration**, edit the **Ratio** column. The **Allocation %** column previews the result. Click **Review changes** → **Apply ratios**. The new ratios apply to blocks created after the change. A block that is partly used finishes with its old ratios, and existing assignments are never changed. You can set an arm's ratio to 0 to stop assigning to it, as long as at least one arm stays positive and the block size (if set) is still a multiple of the new ratio sum. Every ratio change is recorded in **History**.

---

## Access

The **Access** tab lists who can see the study. **Owners** can add people by email (**Grant Access**), change roles, and remove members. **Members** can view. The study must always keep at least one owner. Access takes effect at the person's next login.

> Roles are enforced only on the Access tab today. Any user who can log in to the console can change any study they can open. See [security-concerns.md](security-concerns.md) S1.

---

## When a card does not appear

| What the EHR gets | Cause |
| --- | --- |
| "Randomization unavailable" card | Study is Draft or Paused (production request), or Closed |
| "Unrecognized environment" card | The request's `fhirServer` is not in `FHIR_ENV_MAP` |
| "No locator found" card | Request to `/cds-services/randomization` without Epic's criteria-id extension |
| HTTP 412, no card | Locator not mapped to any study; assigned arm has no locator; or the assigned arm belongs to a different advisory (expected: that patient's card shows on the other arm's advisory) |
| HTTP 500, no card | A required key or stratum value is missing from the request (for example, no `gender` on the Patient) |
