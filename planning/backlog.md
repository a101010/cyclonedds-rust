# Backlog

All future work lives here. The backlog is continually groomed; the story order reflects the
current best understanding of priority.

Conventions:
* A story's title is a short, unique name. The body is a description followed by
  `Depends on`, `Minimal test`, and `Status`.
* **Depends on** lists prerequisite stories that are not yet done.
* **Minimal test** is the smallest runnable check that proves the story is done.
* **Status** is one of `todo`, `in progress`, or `done`. The active story's detailed plan
  lives in `current_story.md`.
* File paths are relative to this repository root. The scope spec is
  `planning/cyclonedds-build-plan.md`. Each story adds its fixtures and tests to
  `cyclonedds-test-suite` (and unit tests to the crate it changes).

## Increment 1 - multifile IDL support

**Goal.** `cyclonedds-build` parses shared IDL as it is authored for to-stations — nested
`dds::` modules, multi-file `#include`/`import`, scoped type references, and `@optional`
members — and generates Rust whose `DdsType::type_name()` matches the name the C `idlc`/C++
registers.

**Scope.**
* In scope: nested-module tree, include/import preprocessor with include dirs and cycle
  detection, `#[dds_typename]` emission, `@optional` -> `Option<T>`, scoped-reference
  resolution, tokenizer literals, fail-loud errors, and the `CompileOptions` additions
  (`include_dirs`, `emit_dds_typename`).
* Out of scope (needs `cyclonedds-derive`): member IDs/extensibility
  (`@id`/`@position`/`@hash_id`, `@final`/`@appendable`/`@mutable`), optional keyed fields.
* Out of scope: bitsets, maps, inheritance, fixed-point, `long double`,
  interfaces/components/valuetypes, `#pragma keylist`.

**Prerequisites.** CMake 3.16+ and a C/C++ compiler (already required by the workspace). The
C `idlc` is built out of tree by `scripts/regen-ops-fixtures.sh` for the parity test.
`cyclonedds-build` is modified in place in this workspace: no fork, no git dependency.

**Done when.** The acceptance gate from `cyclonedds-build-plan.md` holds in this repo: an
IDL shaped like the to-stations input
(`module dds { module hello_world { struct HelloWorldModel { ... }; }; };`, with an included
file and an `@optional` member) compiles through `compile_idl_with_options` into Rust that
builds inside `cyclonedds-test-suite`; the generated `DdsType::type_name()` equals the C
`idlc`-registered name; and a Rust publish/subscribe round-trip of the generated type
succeeds.

### nested-modules
Replace `IdlFile { types, modules }` with a nested definition tree that preserves scope paths,
add a fully-qualified-name walker, and make codegen recurse it into
`pub mod <snake(name)> { use super::*; ... }`. Add `cyclonedds-test-suite/tests/idl/nested_modules.idl`
(`module dds { module hello_world { struct HelloWorldModel { ... }; }; };`) and a test that
generates it and compiles the result.
- **Depends on:** none.
- **Minimal test:** the walker yields `dds::hello_world::HelloWorldModel`; the generated file
  contains `pub mod dds {` / `pub mod hello_world {`; the fixture compiles in the test suite.
- **Status:** todo.

### compile-options-and-cli
Add `include_dirs: Vec<PathBuf>` and `emit_dds_typename: bool` (default `true`) to
`CompileOptions`, thread them into parser/codegen, and expose them through `cyclonedds-idlc`
and `cargo-cyclonedds` (`--include-dir`, `--no-dds-typename`).
- **Depends on:** none.
- **Minimal test:** a `cyclonedds-build` unit test builds options with both fields set; a CLI
  smoke test runs `cyclonedds-idlc` with `--include-dir`.
- **Status:** todo.

### dds-typename-parity
Emit `#[dds_typename("<fq name>")]` from the scope path (gated by `emit_dds_typename`), and add
the differential parity test against the C `idlc` for
`module dds { module hello_world { struct HelloWorldModel; }; };`, plus a Rust pub/sub
round-trip of the generated type.
- **Depends on:** nested-modules, compile-options-and-cli.
- **Minimal test:** `cyclonedds-test-suite/tests/typename_vs_idlc.rs` asserts the generated
  `DdsType::type_name()` equals the idlc-registered name (transcribed via a
  `scripts/regen-typename-fixture.sh`); the round-trip test passes.
- **Status:** todo.

### includes
Add `src/preprocessor.rs` to expand `#include "..."` / `#include <...>` and `import`, search
`include_dirs` then the including file's directory, detect cycles, and feed the combined
source to the parser; teach the tokenizer to accept `#`. Add include fixtures under
`cyclonedds-test-suite/tests/idl/`.
- **Depends on:** compile-options-and-cli.
- **Minimal test:** `a.idl` includes `b.idl`; compiling `a.idl` yields types from both files;
  a cyclic include returns `Err`.
- **Status:** todo.

### literals-optional-failloud
Accept float and hex literals in the tokenizer; emit `Option<inner>` for `@optional` fields
and error on `@key @optional`; resolve scoped type references to correct Rust paths; return an
error naming unsupported constructs instead of silently skipping. Add an `@optional` fixture
and round-trip test to `cyclonedds-test-suite`.
- **Depends on:** nested-modules.
- **Minimal test:** `@optional long x;` generates `pub x: Option<i32>`; `@key @optional` and an
  unsupported construct return `Err`; a cross-module reference generates a resolvable path and
  the optional fixture round-trips.
- **Status:** todo.

### docs-changelog
Update `cyclonedds-build/README.md`, `cyclonedds-idlc/README.md`,
`cargo-cyclonedds/README.md`, the root `README.md`, and `CHANGELOG.md` under `[Unreleased]`.
- **Depends on:** dds-typename-parity, includes, literals-optional-failloud.
- **Minimal test:** `cargo doc --workspace --no-deps` builds; the CHANGELOG entry is present.
- **Status:** todo.
