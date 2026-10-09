# AGENTS.md

Safe, idiomatic Rust bindings for Eclipse CycloneDDS (a C library). A Cargo workspace;
the interesting failure mode is memory corruption in the FFI boundary, so treat `unsafe`
and the ABI carefully.

## Branch scope (`multifile`)

The authoritative scope for this branch is `planning/cyclonedds-build-plan.md`: fork
`cyclonedds-build` to add nested-module parsing, multi-file `#include`/`import`,
`#[dds_typename]` emission, and `@optional` member support. 
## Commands

```bash
# Build
cargo build --workspace --features async

# Tests — MUST be single-threaded. CycloneDDS global domain state SIGSEGVs in parallel.
cargo test --workspace --features async -- --test-threads=1

# Lint (CI gate; missing_docs is allowed)
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings -A missing_docs

# Unsafe inventory gate (see "Unsafe policy")
bash scripts/ci/check-unsafe-contracts.sh
```

CI also runs an AddressSanitizer job on nightly against a fixed subset of
`cyclonedds-test-suite` tests (`RUSTFLAGS=-Zsanitizer=address`,
`ASAN_OPTIONS=detect_leaks=0`). Reproduce locally before touching FFI/loans/callbacks.

MSRV is **1.85** (`rust-version` in root `Cargo.toml`). `--locked` is used in CI, so commit
`Cargo.lock` when dependencies change.

## Workspace layout

| Crate | Role |
|-------|------|
| `cyclonedds` | Safe Rust API (main crate). Modules in `cyclonedds/src/`. |
| `cyclonedds-rust-sys` | Unsafe FFI bindings. Do not hand-edit generated bindings. |
| `cyclonedds-derive` | Proc-macros (`DdsType`, `DdsEnum`, `DdsUnion`, `DdsBitmask`). |
| `cyclonedds-build` | `build.rs` helper (`compile_idl`). |
| `cyclonedds-idlc` | Standalone IDL-to-Rust CLI. |
| `cyclonedds-cli` | CLI tools (16 subcommands, defined in `cyclonedds-cli/src/main.rs`). |
| `cargo-cyclonedds` | Cargo plugin wrapper. |
| `cyclonedds-test-suite` | Integration tests (not published). |
| `cyclonedds-bench` | Criterion benchmarks (not published). |
| `cyclonedds-src` | Bundled CycloneDDS C source (git submodule). |
| `cyclonedds-wasm`, `dds-wasm-*`, `dds-battleship` | Experimental WASM (JSON-over-WebSocket, not real DDS). |
| `fuzz/` | Separate `cargo-fuzz` workspace, NOT a root member. |

## Build gotchas (cyclonedds-rust-sys)

- `build.rs` does **not** run `bindgen`. It uses the checked-in
  `src/prebuilt_bindings.rs` and links a native `ddsc` built via CMake from, in order:
  `CYCLONEDDS_SRC`, the `cyclonedds-src` crate, `vendor/cyclonedds/`, then a system lib
  (system-lib-only is unsupported and panics).
- An **ABI probe** compiles and runs a C program to measure struct layouts, then `const`
  asserts in `src/lib.rs` fail the build on mismatch. It can only run when `HOST == TARGET`.
- **Cross-compiling** requires a snapshot at `cyclonedds-rust-sys/abi/<triple>.rs` and
  panics if missing. Only `x86_64-pc-windows-msvc` is committed; adding a target means
  building natively once and committing the snapshot. Never hand-write a snapshot.
- CMake 3.16+ and a C/C++ compiler are required.

## Unsafe policy (enforced)

`scripts/ci/unsafe-inventory.txt` is the reviewed ceiling of `unsafe` occurrences. CI
recomputes and rejects any drift. New `unsafe` must, in the same PR:
1. sit behind the smallest safe API boundary;
2. carry a nearby `// SAFETY:` contract (pointer validity, lifetime, aliasing, alignment,
   initialization, ownership, unwind, thread-safety as applicable);
3. add a regression test through the public safe API;
4. run pure-Rust paths under Miri and FFI/loans/callbacks/Dynamic XTypes under ASan.

Then update the inventory intentionally. See `SAFETY.md` and `docs/soundness-backlog.md`.

## Conventions

### Planning vs implementing
- Permission to write planning documents is not permission to implement the plan.
  Writing or editing `planning/` allows only those documents to be written.
- Implementing the plan — creating or editing crate sources (`cyclonedds/`,
  `cyclonedds-*`), build files, or generated files — is a separate step and must not be
  combined with a planning step.
- Only permission to implement the plan is permission to implement the plan.

### Version control
- The agent never commits, amends, pushes, or creates pull requests.
- The agent never asks whether to commit; the user decides when to commit.
- The agent may run read-only git commands (`git status`, `git diff`, `git log`) and must
  leave all changes in the working tree for the user to review and commit.

### Planning
- Planning lives in `planning/`: `backlog.md` (all work, prioritized) and
  `current_story.md` (the detailed plan for the one active story). Read them before
  writing code.
- `backlog.md` is continually groomed; story order reflects the current best understanding
  of priority, and the active story's status is tracked there.
- A story's title is a short, unique name; do not restate it as a longer name. The body is
  a description followed by `Depends on`, `Minimal test`, and `Status`.
- IDs, topic names, and file paths are specified in `planning/backlog.md`; use them rather
  than inventing your own.

### Documentation
- Do not duplicate the planning documents in this file; refer to them instead of restating
  their content.
- Never put generated or build files in `planning/`.

### Other project conventions
- **Commits:** Conventional Commits (`type(scope): subject`). Scopes: `sys`, `api`,
  `derive`, `cli`, `build`, `idlc`, `bench`, `wasm`, `ci`, `deps`.
- **CHANGELOG.md:** update under `[Unreleased]` for user-visible changes.
- **ADRs:** irreversible decisions (new external deps, breaking API changes, MSRV bumps)
  go in `docs/adr/` (`CONTRIBUTING.md` has the template). Check whether the directory
  exists before assuming a numbering.
- Public API items get `///` doc comments.
- Do NOT add comments unless asked (global instruction); `// SAFETY:` contracts are the
  required exception.

## Reference docs

`README.md` (feature flags, build details), `CONTRIBUTING.md` (workflow, ADR template),
`SAFETY.md`, `docs/` (architecture, async patterns, type system, QoS, security),
`planning/` (the `multifile` branch scope and planning docs).
