# Current story: docs-changelog

This file is the detailed, living plan for the one active story. It is rewritten for each
story. The backlog in `backlog.md` holds all work and the per-story status.

## Story

Document the `multifile` increment in `cyclonedds-build/README.md`, `cyclonedds-idlc/README.md`,
`cargo-cyclonedds/README.md`, the root `README.md`, and `CHANGELOG.md` under `[Unreleased]`.

- **Depends on:** dds-typename-parity, includes, literals-optional-failloud.
- **Minimal test:** `cargo doc --workspace --no-deps` builds; the CHANGELOG entry is present.

## Decisions

* **Scope is the five files the backlog lists.** `docs/*.md` is not touched here.
* **No ADR.** The `CompileOptions` field addition is a breaking change but is documented only in
  `CHANGELOG.md` (under `### Changed`), per the chosen scope.
* **CHANGELOG structure.** Add `### Added` and `### Changed` before the existing `### Fixed`, and
  append a `### Fixed` bullet for the tokenizer and the derive.
* **Fix a stale example:** `cyclonedds-build/README.md` showed `cyclonedds-build = "2.0"`; the
  crate is `3.0.0`, so it is corrected to `"3.0"`.
* **Document the new options/flags** wherever the CLIs are described: `--include-dir <DIR>`
  (repeatable) and `--no-dds-typename`.
* **Root README:** add a short "IDL Code Generation" note and refresh the
  `cyclonedds-build`/`cyclonedds-idlc` crate-table rows.

## Deliverables

1. `CHANGELOG.md` - `[Unreleased]` entries.
2. `cyclonedds-build/README.md` - capabilities, `CompileOptions` fields, version example.
3. `cyclonedds-idlc/README.md` - new flags + examples.
4. `cargo-cyclonedds/README.md` - new flags + examples.
5. `README.md` - IDL code-generation note + crate-table rows.

## Tests

* `cargo doc --workspace --no-deps` builds.
* `grep -n "include_dirs\|emit_dds_typename\|Option<String>" CHANGELOG.md` finds the entries.

## Minimal test

```
cargo doc --workspace --no-deps
grep -n "### Added" CHANGELOG.md
```

## Result

Done and verified. `CHANGELOG.md` `[Unreleased]` gained `### Added`, `### Changed`, and
`### Fixed` entries for the increment (nested modules, `#include`/`import`, `#[dds_typename]`,
`@optional`, scoped refs; the breaking `CompileOptions` field addition under Changed; tokenizer
literals and the `Option<String>` derive fix under Fixed). No ADR (documented in the changelog
only). The `cyclonedds-build` README documents the new capabilities and `CompileOptions` fields
and its example version is corrected to `3.0`; the `cyclonedds-idlc`/`cargo-cyclonedds` READMEs
document `--include-dir` and `--no-dds-typename`; the root README gets an "IDL Code Generation"
section and refreshed crate rows.

`cargo doc --workspace --no-deps` builds.

## Files

* Authored/changed: `CHANGELOG.md`, `cyclonedds-build/README.md`, `cyclonedds-idlc/README.md`,
  `cargo-cyclonedds/README.md`, `README.md`.
* Planning: `planning/current_story.md`, `planning/backlog.md`.
