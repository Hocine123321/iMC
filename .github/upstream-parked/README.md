# Parked upstream CI

These are Horizon's own GitHub Actions workflows and CODEOWNERS, moved out of `.github/workflows/` when this repository was created. They are **not** run here, on purpose:

- they target the upstream project's self-hosted runners and variables (`SELF_HOSTED_CI`, `MAC_RUNNER`);
- several use upstream secrets, release signing keys and scheduled jobs (nightly builds, Mojang-related republishing) that must not run in a fork;
- some use `pull_request_target`, which is unsafe to enable casually on a public repository.

They are kept unmodified for reference and for rebasing on upstream. This project's own workflows live in `.github/workflows/` (see `ios.yml`).
