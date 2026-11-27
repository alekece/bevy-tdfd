# Skip Rust CI for Documentation-Only Changes

Issue: [#6](https://github.com/alekece/bevy-tdfd/issues/6)

## Intent

Avoid running Rust formatting, Clippy, and tests when a change only updates documentation.

## Goals and non-goals

- Skip the CI workflow for changes consisting only of Markdown files or files under `docs/`, on pull requests and pushes to `main`.
- Run the existing Rust jobs for mixed changes and every other file type, including assets, Cargo files, Rust sources, and workflow configuration.
- Do not add a path-classifier job or third-party action.

## Approaches

- **Workflow `paths-ignore` (chosen):** smallest change and enough for this repository, which has no required status checks configured. A mixed change still triggers the workflow.
- **Classifier job and an always-running gate:** preserves a status for docs-only changes if CI becomes a required check, but adds workflow complexity for a future branch-protection setup that does not currently exist.

The user chose `paths-ignore` to keep this change proportional to the current repository.
