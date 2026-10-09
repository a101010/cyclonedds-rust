# cyclonedds-build fork plan

Work described here is done in a separate repository (the fork). This document is the spec for that repository.

## Purpose

Fork `cyclonedds-build` 3.0.1 (mzet97/cyclonedds-rust) and extend its IDL parser and Rust code generator so to-stations can author full, shared IDL (nested `dds::` modules, multi-file includes, DDS type-name parity with `idlc`/C++, optional members) and generate Rust types for the `cyclonedds` crate. Keep the existing `compile_idl`/`CompileOptions` API shape.

## Gaps this fork closes

The stock 3.0.1 codegen:

1. cannot parse nested modules (`IdlType` has no `Module` variant);
2. skips `#include`/`import` (and `#` fails tokenization) so files cannot share types;
3. names the DDS type after the Rust struct only, never emitting `#[dds_typename]`;
4. drops every annotation except `@key` and `@bit_bound`, so `@optional` is ignored and the plain type is emitted;
5. flattens scoped type references to the last segment;
6. silently skips unknown constructs.

## Fork mechanics

- Fork the upstream workspace repo `mzet97/cyclonedds-rust` (which contains `cyclonedds`, `cyclonedds-build`, `cyclonedds-derive`, `cyclonedds-rust-sys`, `cyclonedds-src`). `cyclonedds-build` is modified in place in the fork; no new nested repo is created.
- Consumption: a pinned git dependency on the fork's repo (`cyclonedds-build = { git = "<fork-url>", rev = "<sha>" }`), or a path dependency during development. Cargo resolves the package by name from the workspace.
- License: keep MIT (upstream). Retain copyright/attribution.
- Versioning: track the upstream base (`3.0.x`) and document the fork base and added features.
- Upstreaming: optional; keep changes minimal and upstreamable, but the fork is the source of truth for to-stations.
- Later: if `@id`/member IDs, type-level extensibility, or optional keyed fields are needed, modify `cyclonedds-derive` in the same fork.

## Scope

### In scope (needed for increment 1 and near-term IDL)

- Parse nested `module` blocks and preserve the full scope path.
- Resolve `#include "file.idl"` / `#include <file.idl>` and `import`, with include directories and cycle detection (preprocessor pass before parsing).
- Emit **nested Rust modules** matching the IDL scopes (e.g. `pub mod dds { pub mod hello_world { ... } }`).
- Emit `#[dds_typename("<scope>::<Name>")]` on topic structs so `DdsType::type_name()` equals the `idlc`/C++ type name (e.g. `dds::hello_world::HelloWorldModel`).
- Emit `Option<inner>` for `@optional` fields (the derive already supports optional members); reject `@optional` on `@key` fields with a clear error.
- Preserve `@key` -> `#[key]`; keep `@bit_bound` for bitmasks.
- Resolve cross-module/scoped type references to correct Rust paths (not last-segment).
- Tokenizer: accept `#` (preprocessor), hex integer literals (`0x...`), and not choke on float literals.
- Fail loudly on unsupported constructs instead of silently skipping.

### Out of scope (deferred; require forking `cyclonedds-derive` too)

- Explicit member IDs (`@id`/`@position`/`@hash_id`) and type-level extensibility (`@final`/`@appendable`/`@mutable`) - the derive has no attributes for these.
- Optional keyed fields (`@key @optional`) - the derive rejects them ("optional keyed fields are not supported yet").
- Bitsets, maps, inheritance, fixed-point, `long double`, interfaces/components/valuetypes.
- `#pragma keylist` (use `@key`).

## Design

### Parser (`src/idl_parser.rs`)

- Replace `IdlFile { types, modules: HashMap<String, Vec<IdlType>> }` with a tree: `IdlFile { definitions: Vec<Definition> }`, `Definition::Module { name, definitions }` plus the existing struct/enum/union/bitmask/typedef variants. Keep a helper that walks the tree and yields fully-qualified names (`dds::hello_world::HelloWorldModel`).
- Preprocessor pass (new `src/preprocessor.rs`): expand `#include`/`import` by inlining files (search `include_dirs` then the including file's directory), track visited files for cycle detection, and feed the combined source to the tokenizer.
- Tokenizer: handle `#` lines, hex (`0x..`) and decimal literals, and floating-point literals as distinct tokens (or skip them where only annotations/consts use them).
- Keep field annotations (including `@optional`) on `IdlField`; the tree preserves the enclosing module scope for each type.

### Codegen (`src/codegen.rs`)

- Recurse modules: emit `pub mod <snake(name)> { use super::*; ... }` per IDL module.
- On structs, emit `#[dds_typename("<fq name>")]` (from the scope path). Gate with an option (default on).
- For a field with `@optional`, emit `Option<inner>`; error if the field is also `@key`.
- Emit `#[key]` for `@key` fields (as today).
- Emit item-level `#[allow(...)]` instead of a crate-level `#![allow(...)]` so generated files are `include!`-friendly inside a module.
- Resolve named type references to the correct Rust path using the fully-qualified IDL name (fall back to the simple name when the type is in the current module).

### Public API (`src/lib.rs`)

Keep `compile_idl(path)` and `compile_idl_with_options(path, &CompileOptions)`. Extend `CompileOptions`:

```
pub struct CompileOptions {
    pub cyclonedds_home: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub try_idlc: bool,
    pub module_name: Option<String>,        // output file stem (unchanged)
    pub include_dirs: Vec<PathBuf>,         // NEW: #include search path
    pub emit_dds_typename: bool,            // NEW: default true
}
```

`module_name` keeps its current meaning (output file stem / header comment). The IDL itself defines the module tree.

## Type-name / interop correctness

- Confirm the exact string `idlc`/`cyclonedds-cxx` registers as the DDS type name for `module dds { module hello_world { struct HelloWorldModel; }; };` (expected `dds::hello_world::HelloWorldModel`), and emit exactly that in `#[dds_typename]`.
- Verification spike: build a Rust type with the fork and a C++/idlc type from the same IDL; confirm CycloneDDS matches the topic types (same registered type name) and samples flow. This is the acceptance gate for the fork.

## Testing

- Unit tests: nested modules, include resolution + cycles, scoped references, `#[dds_typename]` emission, `@key`, `@optional` -> `Option<T>` (and the `@key @optional` error), hex literals, and error-on-unsupported.
- Conformance corpus: the to-stations `DDS/*.idl` files; assert codegen succeeds and the generated `type_name()` values equal the expected scoped names.
- Cross-check test (optional CI): run `idlc -l c` on the same IDL and compare the registered type name to the Rust `type_name()`.
- Optional-member spike: confirm an `@optional` member round-trips between Rust and idlc/C++, including whether the derive's default type-level extensibility is accepted by CycloneDDS.

## Consumption in to-stations

- `rust/Cargo.toml` / the `stations-dds` crate's `build.rs`: depend on the fork (git rev or path) and call `compile_idl_with_options` with `include_dirs = ["DDS"]` and `output_dir = build/gen/rust`.
- The `idl-codegen` story (D5) is updated to use the fork; the `hello-world-topic` story and later IDL stories author `module dds { module <format> { ... } }`.
- Pin the fork rev in `docs/versions.md`.

## Milestones

1. Fork repo + CI; parser tree for nested modules; codegen nested modules (no includes).
2. `#[dds_typename]` emission + the interop spike (Rust <-> idlc/C++ type match).
3. Preprocessor `#include`/`import` with include dirs + cycle detection.
4. Tokenizer literals (`#`, hex, floats), `@optional` -> `Option<T>`, and fail-loud errors.
5. Conformance corpus over `DDS/*.idl`; pin the rev in to-stations.

## Risks

- Exact `idlc` type-name string may differ (separators, casing); the interop spike settles it before the fork is relied upon.
- Cross-module reference path resolution can be subtle; covered by tests.
- Optional members may require type-level extensibility the derive does not emit; the optional-member spike settles it (fallback: keep `@optional` out until the derive is forked).
- Keeping the fork upstreamable may constrain refactors; acceptable trade-off.
- Fork maintenance: upstream may change; track the base version and diff.

## Open questions

- Git dependency vs vendoring the fork in-repo vs a private registry.
- Whether to also fork `cyclonedds-derive` later for `@id`/member IDs, type-level extensibility, and optional keyed fields.
