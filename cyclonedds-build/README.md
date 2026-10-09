# cyclonedds-build

[![crates.io](https://img.shields.io/crates/v/cyclonedds-build.svg)](https://crates.io/crates/cyclonedds-build)

`build.rs` helper that compiles OMG IDL files into Rust source using [`cyclonedds-derive`](https://crates.io/crates/cyclonedds-derive) proc-macros (`DdsTypeDerive`, `DdsEnumDerive`, `DdsUnionDerive`, `DdsBitmaskDerive`).

It first tries to shell out to the CycloneDDS C `idlc` compiler (found via `CYCLONEDDS_HOME/bin/idlc` or `PATH`) to obtain a full type descriptor; if `idlc` is not available, it falls back to a built-in, simplified IDL parser ([`src/idl_parser.rs`](src/idl_parser.rs)).

The built-in parser handles nested `module` blocks, multi-file `#include`/`import`, scoped type references, and `@optional` members, and emits `#[dds_typename("<scope>::<Name>")]` so `DdsType::type_name()` matches the name the C `idlc`/C++ registers.

This crate is the engine behind both [`cyclonedds-idlc`](https://crates.io/crates/cyclonedds-idlc) (the standalone CLI) and [`cargo-cyclonedds`](https://crates.io/crates/cargo-cyclonedds) (the Cargo plugin).

## Usage

```toml
[build-dependencies]
cyclonedds-build = "3.0"
```

```rust
// build.rs
fn main() {
    cyclonedds_build::compile_idl("src/types.idl").unwrap();
}
```

```rust
// src/lib.rs or a module
include!(concat!(env!("OUT_DIR"), "/types.rs"));
```

For more control, use `compile_idl_with_options` with a `CompileOptions` value instead of `compile_idl`. Its fields are `output_dir`, `module_name`, `cyclonedds_home`, `try_idlc`, `include_dirs` (search path for `#include`/`import`), and `emit_dds_typename` (emit `#[dds_typename]`, default `true`).

## Documentation

- [docs.rs/cyclonedds-build](https://docs.rs/cyclonedds-build)
- [Repository](https://github.com/mzet97/cyclonedds-rust)

## License

MIT — see [LICENSE-MIT](https://github.com/mzet97/cyclonedds-rust/blob/main/LICENSE-MIT).
