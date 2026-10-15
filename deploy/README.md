# POCR release bundle

These files install POCR on one Linux host with Docker:

- `compose.yaml`: the six containers. You should not need to edit it.
- `.env.example`: every site setting, explained. `pocr-init` copies it to `.env`.
- `pocr-init`: creates secrets, `.env`, and the data volumes, then lists what is left to do.

Follow the install guide, `docs/install.md` in the POCR repository.
