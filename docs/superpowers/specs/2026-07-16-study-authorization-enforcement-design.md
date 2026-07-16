# Study Authorization Enforcement — Design

**Date:** 2026-07-16
**Status:** Approved (ready for planning)

## Problem

Vouch authenticates identity and `/api/me` surfaces the user's `groups` (e.g.
`ctsi-randomization-admins`), but **no authorization is enforced server-side**.

- `create_study` records the creator as `"owner"` in `user_studies`, and
  `list_studies` scopes by `list_for_user` — these are correct.
- **Every other study endpoint trusts any authenticated header.**
  `get_study`, `update_study`, `transition_study`, `delete_study`,
  `purge_nonproduction`, `set_arm_locator`, and `share_study` never check
  membership. Any UCSF user who can reach the API can read or mutate any study
  by UUID.
- `list_randomizations` and `get_balance` don't even take `AuthUser` — no check
  at all.
- Vouch `groups` are surfaced to the frontend but never enforced.
- `UserStudyRepository` has no "what is this user's role for this study?"
  lookup; roles are freeform strings; `ApiError` has no `Forbidden` (403).

For a clinical-trial system this is the highest-risk gap: unauthorized read of
allocation data and unauthorized mutation of live studies.

## Decisions (locked)

- **Model:** membership + role hierarchy + admin group. Per-study roles
  (`Owner > Member`) gate actions; the Vouch admin group grants a global
  override.
- **Permission split:** Owner mutates, Member reads. Owner (and admin) may
  update, transition, delete, purge, set-locator, and share. Member (and admin)
  may read: get, list randomizations, balance.
- **Deny response:** `403 Forbidden` (explicit; existence not hidden).
- **Mechanism:** authentication stays in the extractor; authorization is an
  explicit, pure decision function called where the resource is loaded. No
  typed role extractors (would double-fetch and couple to path shape), no tower
  middleware (can't see the resource).
- **No DB migration:** the existing `user_studies.role` text column maps to the
  new `StudyRole` via `FromStr`; unknown values fail closed.

## Section 1 — Domain model (`core::authz`, new module)

```rust
pub enum StudyRole { Owner, Member }   // ordered: Owner > Member

pub enum Action { Read, Write, Delete, Share }  // Write = update/transition/set-locator/purge

pub struct Principal {
    pub user_id: String,
    pub is_admin: bool,   // derived from Vouch groups
}

/// Pure decision. No I/O. Membership is looked up by the caller and passed in.
pub fn authorize(principal: &Principal, membership: Option<StudyRole>, action: Action) -> bool
```

Rules:

- Admin ⇒ always allow.
- Otherwise: `Read` needs any membership; `Write` / `Delete` / `Share` need
  `Owner`.
- Non-member (`None`) ⇒ deny.

`StudyRole` gets `FromStr` / `as_str` mapping to `"owner"` / `"member"`. Unknown
role strings are treated as no membership (fail closed).

## Section 2 — Ports & repository

`UserStudyRepository` gains:

```rust
/// Return the user's role for a study, or None if not a member.
async fn role_for(&self, user_id: &str, study_id: &Uuid)
    -> Result<Option<StudyRole>, RepositoryError>;
```

PG impl: `SELECT role FROM user_studies WHERE user_id=$1 AND study_id=$2`, parse
via `StudyRole::from_str`; an unrecognized value logs a warning and returns
`None`. One round-trip per request; no double-fetch of the study.

## Section 3 — Extractor & admin group

`AuthUser` extends to carry Vouch groups and expose a `Principal`:

```rust
pub struct AuthUser { pub id: String, pub groups: Vec<String> }
impl AuthUser {
    pub fn principal(&self, admin_group: &str) -> Principal; // is_admin = groups.contains(admin_group)
}
```

- Groups parsed from `X-Vouch-Groups` (same header `/api/me` already parses).
- Admin group name is config on `AppState` (env var `ADMIN_GROUP`, default
  `ctsi-randomization-admins`).
- Debug-mode fallback stays; the fake dev user is given the admin group, keeping
  parity with today's `/api/me` behavior.

## Section 4 — Handler enforcement & error

Add `ApiError::Forbidden` → `403`.

Handler helper keeps enforcement to one line each:

```rust
async fn require(state, principal, study_id, action) -> Result<(), ApiError>;
```

- **Reads** (`get_study`, `list_randomizations`, `get_balance`): now take
  `AuthUser`; require `Read`. `list_randomizations` / `get_balance` gain the
  `AuthUser` param they currently lack.
- **Writes** (`update_study`, `transition_study`, `purge_nonproduction`,
  `set_arm_locator`): require `Write`.
- **Delete** (`delete_study`): require `Delete`; load the study first (today it
  deletes blind).
- **Share** (`share_study`): require `Share`.
- **`join_study`**: unchanged (token is the grant); hardcoded `"member"` maps to
  `StudyRole::Member`.
- **`create_study`, `list_studies`**: unchanged.

## Section 5 — Testing

- `core::authz` unit tests: full matrix {admin, owner, member, none} ×
  {read, write, delete, share}.
- `StudyRole::from_str` including unknown-string-fails-closed.
- Handler tests for the deny path (403) on a non-member and the admin-override
  allow path, using the crate's existing in-memory/mock repos.

No DB migration. No required frontend change (frontend already reads `groups`
from `/api/me` for display; server enforcement is the source of truth).
