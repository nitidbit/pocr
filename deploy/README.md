# POCR release bundle

These files install POCR on one Linux host with Docker:

- `compose.yaml`: the six containers. You should not need to edit it.
- `.env.example`: every site setting, explained. `pocr-init` copies it to `.env`.
- `pocr-init`: creates secrets, `.env`, the data volumes, and the TLS certificate, then lists what is left to do.
- `pocr-backup`: backs up the database, and tests that a backup restores.
- `pocr-upgrade`: upgrades to another release.
- `docs/`: the documentation for this release.

Follow the install guide, `docs/install.md`. Send `docs/epic-integration.md` to your Epic analysts and `docs/study-setup.md` to study teams.
