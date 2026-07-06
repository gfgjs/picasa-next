# Contributing

Thanks for your interest in contributing! This document sets expectations so your time is well spent.

## Scope: what is open, what is not

The open-source core of this project (everything in this repository) is licensed under **Apache-2.0**.

The following are **closed-source commercial products** and are *not* developed in this repository:

- The AI inference plugin (encrypted model weights and its licensing/entitlement backend)
- The face-recognition plugin
- The exotic-format plugins (e.g. PSD engine) in their commercially distributed, signed form
- The production signing/licensing infrastructure (trust roots, key material, issuance services)

**We do not accept pull requests that implement, re-implement, or modify the paid-plugin feature set or its entitlement/licensing paths.** Such PRs will be closed with a pointer to this document. Bug reports against paid plugins are welcome in the issue tracker.

The open-source build compiles a fully functional free application; paid features resolve to a free stub by design.

## Contributor License Agreement (CLA)

Before we can merge your first pull request, you must sign the project CLA (see [CLA.md](CLA.md)). A bot will prompt you on your first PR. The CLA is the standard Apache-style individual CLA: you keep your copyright and grant the project a copyright and patent license to your contribution, including the right to relicense.

## Trademarks

The project name and logo are trademarks and are **not** licensed under Apache-2.0. See [TRADEMARK.md](TRADEMARK.md). Forks must use a different name and logo.

## Practical notes

- Rust: `rustfmt` + `clippy -D warnings` must pass. Frontend: ESLint + Prettier + `vue-tsc` strict.
- All SQL goes through parameter binding; no string concatenation.
- Core logic changes need unit tests; CI runs the full suite on every PR.
- Comments in the codebase are predominantly in Chinese (project convention); either language is fine in PRs.
