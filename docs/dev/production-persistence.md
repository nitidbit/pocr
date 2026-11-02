# POCR Production Persistence — Keeping Dockerized Postgres Not-Too-Virtual

**Date:** 2026-08-04
**Context:** Companion to [external-dev-guide.md](external-dev-guide.md). Premise: a production install keeps the dockerized Postgres from `compose.yaml` rather than pointing the engine at an institutionally managed database. That is a legitimate choice — but a Docker volume's default lifecycle is looser than a system-of-record deserves. This doc covers what the current compose file already gets right, the four gaps that remain, and the operational rules an installing site should follow.

---

## What the current setup already does

Production compose puts Postgres on a named volume — `pg_data:/var/lib/postgresql` (`compose.yaml`) — so the data survives container restarts, image rebuilds, `docker compose up -d --force-recreate`, and plain `docker compose down`. The data is real files on the host, at `/var/lib/docker/volumes/<project>_pg_data/_data`.

Two details already handled, for the record:

- The mount targets `/var/lib/postgresql` (the parent), not `.../data`, because the PG18+ images use a version-specific subdirectory — see the comment in `compose.dev.yaml` and docker-library/postgres#1259.
- The image is pinned to a major version (`postgres:18-alpine`), so an unattended `docker compose pull` cannot silently jump major versions.

"Survives normal operations" is not the same as "safe," though. Four gaps follow, roughly in order of how likely each is to bite.

## Gap 1 — Routine Docker hygiene can delete the volume

As declared, `pg_data` is owned by the Compose project. That means:

- `docker compose down -v` destroys it, silently, no confirmation.
- `docker volume rm <project>_pg_data` works any time the stack is stopped.

**Fix: declare the volume external** so Compose can never delete it:

```yaml
volumes:
  pg_data:
    external: true
    name: pocr_pg_data
  redis_data:
    external: true
    name: pocr_redis_data
```

Creating the volumes becomes a one-time install step, done before first `docker compose up`:

```sh
docker volume create pocr_pg_data
docker volume create pocr_redis_data
```

From then on `down -v` leaves the volumes alone (it skips external volumes without an error; verified 2026-10-14 with Compose v5.1). (`docker volume rm` still works when the stack is down — Docker offers no true deletion lock — but external status removes every *accidental* path.)

**Alternative: bind mount an explicit host path**, e.g. `/srv/pocr/pgdata:/var/lib/postgresql`. The data then lives somewhere visible to whoever manages the box and to whatever backup policy already covers host paths. Choose this when the institution's ops team backs up directories, not Docker volumes. The external named volume is the more docker-native default; pick one and document the choice in the site's runbook.

## Gap 2 — The volume name is coupled to the checkout directory name

Compose prefixes volume names with the project name, which defaults to the containing directory. Deploy from a folder named `pocr-prod` instead of `pocr`, or rename the directory later, and Compose quietly creates a brand-new empty `pg_data`. Postgres initializes a fresh cluster on it, and the app comes up with zero studies and zero allocations — indistinguishable from data loss until someone finds the old volume.

**Fix:** pin the project name at the top of `compose.yaml`:

```yaml
name: pocr
```

The `external: true` + explicit `name:` approach from Gap 1 sidesteps this for the volumes specifically, but pinning the project name also stabilizes network and container names, so do both.

## Gap 3 — Persistence is not durability: there are no backups

A volume protects against nothing that happens *to the host*: disk failure, filesystem corruption, a bad migration, an accidental `DELETE`, a compromised box. For a randomization app the allocation history is the crown jewel — it is the audit trail that proves assignment integrity for the trial.

**Minimum bar: nightly logical dumps, stored off the Docker host** (or at the very least outside `/var/lib/docker`), with retention and a tested restore.

Host cron example:

```sh
# /etc/cron.d/pocr-backup — nightly at 02:15, keep 30 days
15 2 * * * pocr cd /srv/pocr && docker compose exec -T postgres \
  pg_dump -U pocr -Fc pocr > /srv/pocr-backups/pocr-$(date +\%F).dump \
  && find /srv/pocr-backups -name 'pocr-*.dump' -mtime +30 -delete
```

Notes:

- `-Fc` (custom format) allows selective/parallel restore via `pg_restore`.
- `-T` matters: without it `exec` allocates a TTY and corrupts the binary output.
- Ship the dump directory to institutional storage (rsync target, object store, whatever the site's policy blesses). A backup that lives only on the database host is a copy, not a backup.
- **Test the restore, on a schedule.** An untested backup is a hope. The drill: `docker volume create pocr_pg_restore_test`, start a throwaway `postgres:18-alpine` against it, `pg_restore -U pocr -d pocr < dump`, sanity-query allocation counts, tear down. Put a calendar entry on it (quarterly is reasonable) and record the date it last succeeded.

**Higher bar — point-in-time recovery:** nightly dumps bound the loss window at 24 hours. If the trial cannot tolerate losing a day of enrollments, add WAL archiving with pgBackRest or wal-g, which allows restore to any moment. This adds real operational surface (archive storage, retention tuning, restore rehearsal is more involved) — proportionate for high-volume enrollment, likely overkill for a trickle. Decide per site based on enrollment rate and tolerance.

## Gap 4 — Major-version upgrades need a documented path

The Postgres data directory format is major-version-specific. Someone bumping `postgres:18-alpine` to `postgres:19-alpine` in good faith will find the container refuses to start on the old data (the PG18+ image's version-suffixed subdirectory layout makes this an error, not corruption — but it still looks like an outage).

Rules for the runbook:

- **Patch/minor bumps** (18.1 → 18.2): safe; pull and recreate.
- **Major bumps** (18 → 19): planned maintenance. Simplest safe path: take a final `pg_dump`, stop the stack, create a fresh volume, start the new major against it, `pg_restore`, verify, then retire the old volume. (`pg_upgrade` is faster for large clusters but needs both binaries in one container image — not worth the ceremony at POCR's data size.)
- Never delete the old volume until the new one has passed verification *and* a post-upgrade backup exists.

## A note on Redis

`redis_data` has the same `down -v` exposure, but Redis holds coordination and dedup state, not the system of record — losing it is an inconvenience (a window of duplicate-request risk on restart), not an incident. Make it external anyway (shown in Gap 1) for consistency and to keep `down -v` from being dangerous in *any* form, but it needs no backup regime.

## Operator checklist (install day)

1. `docker volume create pocr_pg_data && docker volume create pocr_redis_data` (after the `external: true` compose change lands).
2. Confirm `name: pocr` is set in `compose.yaml` (or `COMPOSE_PROJECT_NAME` is pinned in `.env`).
3. Stand up the backup cron; verify the first dump is non-trivial in size and lands off-host.
4. Run the restore drill once, before go-live, and schedule the recurring drill.
5. Write down, wherever the site keeps its runbook: **never run `docker compose down -v` in production**, and major Postgres bumps are a planned migration, not a version edit.
