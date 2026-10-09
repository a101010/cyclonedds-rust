# Current story: dds-typename-parity

This file is the detailed, living plan for the one active story. It is rewritten for each
story. The backlog in `backlog.md` holds all work and the per-story status.

## Story

Add `emit_dds_typename` to `CompileOptions` (default `true`) and `--no-dds-typename` to
`cyclonedds-idlc`/`cargo-cyclonedds`; thread the module scope through codegen and emit
`#[dds_typename("<fq name>")]` on structs when enabled; add the differential parity test
against the C `idlc` and a Rust publish/subscribe round-trip of the generated type.

- **Depends on:** nested-modules.
- **Minimal test:** the generated `dds::hello_world::HelloWorldModel`'s
  `DdsType::type_name()` equals the idlc-registered name (transcribed via
  `scripts/regen-typename-fixture.sh`), and a Rust pub/sub round-trip of that type succeeds.

## Decisions

* **Verify the name first.** The exact idlc-registered string is the one real unknown
  (separators/casing). The first implementation step runs the existing `idlc` on the fixture
  and reads the name; the emission and the test are written against that value, not a guess.
* **`idlc` is not rebuilt.** It is already available (built for to-stations-proto):
  `C:/Libraries/cyclonedds/bin/idlc.exe` (11.0.1, with `cycloneddsidl.dll`/`cycloneddsidlc.dll`
  beside it). `scripts/regen-typename-fixture.sh` locates it via `$IDLC`, then
  `$CYCLONEDDS_HOME/bin/idlc.exe`, then the to-stations prefix, then `PATH`.
* **Codegen API.** `generate_rust(idl_file, module_name)` is kept as a thin wrapper over a new
  `generate_rust_with_options(idl_file, module_name, &CompileOptions)`, so the existing public
  function is not broken. `lib.rs` calls the options version.
* **Scope tracking.** The module path is threaded as a `&[&str]` parameter through
  `generate_definition`/`generate_type`/`generate_struct`/`generate_module`, mirroring
  `IdlFile::scoped_types()`. No name→scope map is used (simple names collide across modules).
* **Attribute placement.** `#[dds_typename(...)]` is emitted after `#[derive(...)]`; the derive
  reads it from `input.attrs` regardless of order.
* **Emission scope.** Emitted on every struct when enabled, including top-level ones (there the
  FQ name equals the simple name, so it is behaviorally neutral).
* `--no-dds-typename` is a boolean flag that sets `emit_dds_typename: false`.
* The existing `tests/nested_modules.rs` assertion (`"HelloWorldModel"`) is updated to the FQ
  name, since emission is now on by default.
* The test keeps the transcribed-expectation pattern (like `ops_vs_idlc.rs`), so `cargo test`
  does not need `idlc` at runtime.

## Deliverables

1. `cyclonedds-build/src/lib.rs` - `emit_dds_typename` field; thread options into
   `parse_and_generate`.
2. `cyclonedds-build/src/codegen.rs` - `generate_rust_with_options`, scope threading,
   `#[dds_typename]` emission, tests.
3. `cyclonedds-idlc/src/main.rs` - `--no-dds-typename`.
4. `cargo-cyclonedds/src/main.rs` - `--no-dds-typename`.
5. `cyclonedds-test-suite/tests/nested_modules.rs` - assertion updated to the FQ name.
6. `cyclonedds-test-suite/tests/typename_vs_idlc.rs` (new) - parity + round-trip.
7. `scripts/regen-typename-fixture.sh` (new) - prints the idlc-registered name.

## Interfaces

### `cyclonedds-build/src/lib.rs`

```
pub struct CompileOptions {
    pub cyclonedds_home: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub try_idlc: bool,
    pub module_name: Option<String>,
    pub emit_dds_typename: bool,   // NEW, default true
}

impl Default for CompileOptions {   // emit_dds_typename: true added
    ...
}

fn parse_and_generate(idl_content: &str, module_name: &str, options: &CompileOptions)
    -> Result<String>;
```

### `cyclonedds-build/src/codegen.rs`

```
use crate::CompileOptions;

pub fn generate_rust(idl_file: &IdlFile, module_name: &str) -> String;              // unchanged signature
pub fn generate_rust_with_options(idl_file: &IdlFile, module_name: &str,
                                  options: &CompileOptions) -> String;              // NEW

fn generate_definition(def: &Definition, scope: &[&str], options: &CompileOptions) -> String;
fn generate_type(ty: &IdlType, scope: &[&str], options: &CompileOptions) -> String;
fn generate_struct(s: &IdlStruct, scope: &[&str], options: &CompileOptions) -> String;
fn generate_module(m: &IdlModule, scope: &[&str], options: &CompileOptions) -> String;
fn qualified_name(scope: &[&str], name: &str) -> String;
```

`generate_enum`/`generate_union`/`generate_bitmask`/`generate_typedef` keep their current
signatures (no scope/name attribute).

### `cyclonedds-idlc/src/main.rs`, `cargo-cyclonedds/src/main.rs`

```
/// Do not emit #[dds_typename(...)] on generated structs.
#[arg(long)]
no_dds_typename: bool,
```

### `cyclonedds-test-suite/tests/typename_vs_idlc.rs` (new)

```
include!(concat!(env!("OUT_DIR"), "/gen/nested_modules.rs"));
#[test] fn type_name_matches_idlc();
#[test] fn generated_type_round_trips();
```

### `scripts/regen-typename-fixture.sh` (new)

```
regen-typename-fixture.sh   # prints the idlc-registered name for the fixture IDL
```

## Pseudocode

### `lib.rs`

```
compile_idl_with_options(path, options):
    content = read(path)
    module_name = options.module_name or stem(path)
    code = if options.try_idlc:
               compile_with_idlc_or_fallback(content, module_name, options)
           else:
               parse_and_generate(content, module_name, options)
    ... write code to output_dir/module_name.rs ...

parse_and_generate(content, module_name, options):
    idl_file = parse_idl(content)?
    return codegen::generate_rust_with_options(&idl_file, module_name, options)
```

### `codegen.rs`

```
generate_rust(idl_file, module_name):
    return generate_rust_with_options(idl_file, module_name, &CompileOptions::default())

generate_rust_with_options(idl_file, module_name, options):
    out  = "// @generated ...\n// Source IDL module: " + module_name + "\n\n"
    out += "#[allow(unused_imports)]\nuse cyclonedds::{DdsTypeDerive, DdsEnumDerive, DdsUnionDerive, DdsBitmaskDerive};\n"
    out += "#[allow(unused_imports)]\nuse cyclonedds::{DdsSequence, DdsBoundedSequence, DdsString};\n\n"
    for def in idl_file.definitions:
        out += generate_definition(def, scope = [], options) + "\n"
    return out

generate_definition(def, scope, options):
    match def:
        Type(ty)  -> generate_type(ty, scope, options)
        Module(m) -> generate_module(m, scope, options)

generate_type(ty, scope, options):
    match ty:
        Struct(s) -> generate_struct(s, scope, options)
        Enum(e)   -> generate_enum(e)
        Union(u)  -> generate_union(u)
        Bitmask(b)-> generate_bitmask(b)
        Typedef(t)-> generate_typedef(t)

generate_struct(s, scope, options):
    out = ITEM_ALLOW
    out += "#[derive(Debug, Clone, Default, PartialEq, DdsTypeDerive)]\n"
    if options.emit_dds_typename:
        out += "#[dds_typename(\"" + qualified_name(scope, s.name) + "\")]\n"
    out += "pub struct " + s.name + " {\n"
    ... fields (unchanged) ...
    out += "}\n"

generate_module(m, scope, options):
    child = scope + [m.name]
    out = ITEM_ALLOW + "pub mod " + snake(m.name) + " {\n    use super::*;\n\n"
    for c in m.definitions:
        out += indent(generate_definition(c, child, options), 4) + "\n"
    out += "}\n"

qualified_name(scope, name):
    if scope is empty: return name
    return join("::", scope) + "::" + name
```

### `cyclonedds-idlc/src/main.rs` / `cargo-cyclonedds/src/main.rs`

```
options = CompileOptions { ..., emit_dds_typename: !args.no_dds_typename }
```

### `cyclonedds-test-suite/tests/typename_vs_idlc.rs`

```
include!(concat!(env!("OUT_DIR"), "/gen/nested_modules.rs"));

// idlc-registered name, from scripts/regen-typename-fixture.sh:
//   dds::hello_world::HelloWorldModel

test type_name_matches_idlc():
    assert <dds::hello_world::HelloWorldModel as DdsType>::type_name()
           == "dds::hello_world::HelloWorldModel"

test generated_type_round_trips():
    dp = DomainParticipant::new(0)?
    topic_name = unique_topic("typename")
    pub_topic = Topic::<dds::hello_world::HelloWorldModel>::new(&dp, topic_name)?
    sub_topic = Topic::<dds::hello_world::HelloWorldModel>::new(&dp, topic_name)?
    writer = DataWriter::new(&Publisher::new(&dp)?, pub_topic)?
    reader = DataReader::new(&Subscriber::new(&dp)?, sub_topic)?
    sample = HelloWorldModel { id: 7, message: "hi".into() }
    wait until matched; writer.write(&sample)?
    wait_for(timeout, reader has a sample)
    taken = reader.take()?
    assert taken[0].id == 7 and taken[0].message == "hi"
```

### `scripts/regen-typename-fixture.sh`

```
idlc = first existing of: $IDLC, $CYCLONEDDS_HOME/bin/idlc.exe,
                          C:/Libraries/cyclonedds/bin/idlc.exe, idlc (PATH)
fail "idlc not found" if none
out = mktemp -d
copy tests/idl/codegen/nested_modules.idl -> out/
run: idlc -l c -S -o out out/nested_modules.idl     # writes generated C
print idlc -v
print the registered type-name line from out/*.c/.h   # for transcription
```

## Tests

* `codegen.rs` (unit):
  * `test_generate_dds_typename` - default options emit
    `#[dds_typename("dds::hello_world::HelloWorldModel")]`.
  * `test_generate_dds_typename_disabled` - `emit_dds_typename: false` emits no `dds_typename`.
  * Existing `test_generate_nested_modules` still passes (now also asserts the attribute).
* `lib.rs` (unit): `test_compile_options_emit_dds_typename` - compile with
  `emit_dds_typename: false` and assert the output has no `dds_typename`.
* `cyclonedds-test-suite`:
  * `tests/nested_modules.rs` - assertion updated to `"dds::hello_world::HelloWorldModel"`.
  * `tests/typename_vs_idlc.rs` - parity assertion + round-trip.
* `cyclonedds-idlc`/`cargo-cyclonedds`: covered by compilation of the new flag (no dedicated
  harness).

## Minimal test

```
# step 1 (once, manual): scripts/regen-typename-fixture.sh  -> capture the registered name
cargo test -p cyclonedds-build
cargo test -p cyclonedds-test-suite --test nested_modules --test typename_vs_idlc -- --test-threads=1
cargo clippy -p cyclonedds-build -p cyclonedds-idlc -p cargo-cyclonedds --all-targets -- -D warnings -A missing_docs
```

## Files

* Authored/changed: `cyclonedds-build/src/lib.rs`, `cyclonedds-build/src/codegen.rs`,
  `cyclonedds-idlc/src/main.rs`, `cargo-cyclonedds/src/main.rs`,
  `cyclonedds-test-suite/tests/nested_modules.rs`,
  `cyclonedds-test-suite/tests/typename_vs_idlc.rs` (new),
  `scripts/regen-typename-fixture.sh` (new).
* Planning: `planning/current_story.md`, `planning/backlog.md`.
