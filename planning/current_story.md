# Current story: nested-modules

This file is the detailed, living plan for the one active story. It is rewritten for each
story. The backlog in `backlog.md` holds all work and the per-story status.

## Story

Replace `IdlFile { types, modules }` with a nested definition tree that preserves scope paths,
add a fully-qualified-name walker, and make codegen recurse the tree into
`pub mod <snake(name)> { use super::*; ... }`. Add
`cyclonedds-test-suite/tests/idl/codegen/nested_modules.idl` and a test that generates it and
compiles the result.

- **Depends on:** none.
- **Minimal test:** the walker yields `dds::hello_world::HelloWorldModel`; the generated file
  contains `pub mod dds {` / `pub mod hello_world {`; the fixture compiles in the test suite.

## Decisions

* `IdlType` is kept unchanged and wrapped by a new `Definition` enum
  (`Definition::Type(IdlType)` / `Definition::Module(IdlModule)`) rather than folding the five
  type variants into `Definition`. This preserves `codegen::generate_type` and every existing
  `IdlType` consumer.
* The parser is refactored around one shared `parse_definition_list(terminator)` routine used
  by both the file body and module bodies; module parsing recurses.
* `scoped_types()` returns a `Vec` (depth-first) rather than a custom iterator; it is used by
  tests now and by `dds-typename-parity` next.
* Generated code is `include!`d at the **crate root** of the test file (not inside a `mod`),
  so the generated `#![allow(...)]` inner attribute stays valid; no codegen change for
  include-friendliness is needed here.
* Codegen fixtures live in a dedicated `cyclonedds-test-suite/tests/idl/codegen/` directory;
  `build.rs` compiles every `*.idl` in that directory. `ops_reference.idl` stays at
  `tests/idl/` and is untouched (it is a hand-transcribed reference for the ops differential
  tests, not a codegen fixture). No skip-list is needed.
* `try_idlc: false` keeps generation deterministic (built-in parser, independent of whether
  `idlc` is installed).
* Duplicate module/type names within one scope are out of scope here (fail-loud is
  `literals-optional-failloud`); the tree preserves them in order.

## Deliverables

1. `cyclonedds-build/src/idl_parser.rs` - tree types, recursive parser, FQ walker, updated
   unit tests.
2. `cyclonedds-build/src/codegen.rs` - recursive module emission, nested-module unit test.
3. `cyclonedds-test-suite/tests/idl/codegen/nested_modules.idl` - codegen fixture.
4. `cyclonedds-test-suite/build.rs` - generates fixtures into `$OUT_DIR/gen/`.
5. `cyclonedds-test-suite/Cargo.toml` - add `[build-dependencies] cyclonedds-build`.
6. `cyclonedds-test-suite/tests/nested_modules.rs` - includes and exercises the generated
   module.
7. `planning/backlog.md` - update the `nested-modules` story fixture path to
   `tests/idl/codegen/nested_modules.idl`.

## Interfaces

### `cyclonedds-build/src/idl_parser.rs`

```
pub struct IdlFile { pub definitions: Vec<Definition> }

pub enum Definition {
    Module(IdlModule),
    Type(IdlType),
}

pub struct IdlModule {
    pub name: String,
    pub definitions: Vec<Definition>,
}

// IdlType and its five variants are unchanged.

impl IdlType {
    pub fn name(&self) -> &str;          // each variant's `.name`
}

pub struct ScopedType<'a> {
    pub scope: Vec<&'a str>,             // module segments, e.g. ["dds", "hello_world"]
    pub ty: &'a IdlType,
}
impl ScopedType<'_> {
    pub fn name(&self) -> &str;          // self.ty.name()
    pub fn qualified_name(&self) -> String;  // "dds::hello_world::HelloWorldModel"
}

impl IdlFile {
    pub fn scoped_types(&self) -> Vec<ScopedType<'_>>;  // depth-first
}

pub fn parse_idl(input: &str) -> Result<IdlFile, String>;   // signature unchanged
```

Removed: `IdlFile::types`, `IdlFile::modules`, and the `use std::collections::HashMap` import.

### `cyclonedds-build/src/codegen.rs`

```
pub fn generate_rust(idl_file: &IdlFile, module_name: &str) -> String;   // signature unchanged
fn generate_definition(def: &Definition) -> String;
fn generate_module(m: &IdlModule) -> String;
fn indent(code: &str, spaces: usize) -> String;
fn generate_type(ty: &IdlType) -> String;   // unchanged
```

### `cyclonedds-test-suite/build.rs` (new)

```
const CODEGEN_DIR: &str = "tests/idl/codegen";
fn main();
```

### `cyclonedds-test-suite/tests/nested_modules.rs` (new)

```
include!(concat!(env!("OUT_DIR"), "/gen/nested_modules.rs"));
use cyclonedds::DdsType;
#[test] fn nested_modules_compile_and_expose_types();
```

## Pseudocode

### Parser

```
parse_file():
    defs = parse_definition_list(terminator = NONE)
    return IdlFile { definitions: defs }

parse_definition_list(terminator):
    defs = []
    while peek() != terminator and peek() != NONE:
        annotations = parse_annotations()
        if let Some(def) = parse_definition(annotations):
            defs.push(def)
    return defs

parse_definition(annotations):
    match peek():
        Ident("module")  -> Some(Module(parse_module()))
        Ident("struct")  -> Some(Type(parse_struct(annotations)))
        Ident("enum")    -> Some(Type(parse_enum(annotations)))
        Ident("union")   -> Some(Type(parse_union(annotations)))
        Ident("bitmask") -> Some(Type(parse_bitmask(annotations)))
        Ident("typedef") -> parse_typedef(annotations).map(Type)
        Ident("const" | "import" | "include" | "type" | "annotation")
                         -> skip_to_semi(); None
        _                -> skip_to_semi(); None

parse_module():
    expect(Ident("module")); name = expect_ident(); expect(LBrace)
    defs = parse_definition_list(terminator = RBrace)
    expect(RBrace)
    if peek() == Semi: advance()          // optional trailing ';'
    return IdlModule { name, definitions: defs }
```

### Walker

```
IdlFile.scoped_types():
    out = []
    collect(self.definitions, scope = [], out)
    return out

collect(defs, scope, out):
    for def in defs:
        match def:
            Type(ty)  -> out.push(ScopedType { scope: scope, ty })
            Module(m) -> collect(m.definitions, scope + [m.name], out)

ScopedType.qualified_name():
    return join("::", self.scope + [self.ty.name()])
```

### Codegen

```
generate_rust(file, module_name):
    out  = "// @generated by cyclonedds-build. DO NOT EDIT.\n"
    out += "// Source IDL module: " + module_name + "\n\n"
    out += "#![allow(unused_imports, dead_code, non_camel_case_types, non_snake_case)]\n\n"
    out += "use cyclonedds::{DdsTypeDerive, DdsEnumDerive, DdsUnionDerive, DdsBitmaskDerive};\n"
    out += "use cyclonedds::{DdsSequence, DdsBoundedSequence, DdsString};\n\n"
    for def in file.definitions:
        out += generate_definition(def) + "\n"
    return out

generate_definition(def):
    match def:
        Type(ty)  -> generate_type(ty)
        Module(m) -> generate_module(m)

generate_module(m):
    out  = "pub mod " + snake(m.name) + " {\n"
    out += "    use super::*;\n\n"
    for child in m.definitions:
        out += indent(generate_definition(child), 4) + "\n"
    out += "}\n"
    return out

indent(code, n):
    pad = " " * n
    return join("\n", pad + line for line in code.lines()) + "\n"
```

### `cyclonedds-test-suite/build.rs`

```
main():
    out = Path(env("OUT_DIR")) / "gen"
    create_dir_all(out)
    println("cargo:rerun-if-changed=" + CODEGEN_DIR)
    for path in sorted(files(CODEGEN_DIR, "*.idl")):
        println("cargo:rerun-if-changed=" + path)
        options = CompileOptions {
            output_dir: Some(out),
            try_idlc: false,
            module_name: Some(stem(path)),
            ..default,
        }
        compile_idl_with_options(path, options)
            .unwrap_or_else(|e| panic("codegen failed for " + path + ": " + e))
```

### `tests/nested_modules.rs`

```
include!(concat!(env!("OUT_DIR"), "/gen/nested_modules.rs"));
use cyclonedds::DdsType;

test nested_modules_compile_and_expose_types():
    assert <dds::hello_world::HelloWorldModel as DdsType>::type_name() == "HelloWorldModel"
```

## Fixture

`cyclonedds-test-suite/tests/idl/codegen/nested_modules.idl`:

```idl
module dds {
  module hello_world {
    struct HelloWorldModel {
      @key long id;
      string message;
    };
  };
};
```

## Tests

* `idl_parser.rs`:
  * `test_parse_nested_modules` - tree shape (`Definition::Module` -> `Definition::Module` ->
    `Definition::Type`) and `scoped_types()[0].qualified_name() ==
    "dds::hello_world::HelloWorldModel"`.
  * Update `test_parse_simple_struct`, `test_parse_module`, `test_parse_cross_module_reference`,
    `test_parse_typedef_array`, `test_parse_bitmask`, `test_parse_nested_struct` to read
    `file.definitions` instead of `file.types` / `file.modules`.
* `codegen.rs`:
  * `test_generate_nested_modules` - output contains `pub mod dds {`,
    `pub mod hello_world {`, and `pub struct HelloWorldModel`.
  * `test_generate_simple_struct` unchanged (leaf path).
* `cyclonedds-test-suite/tests/nested_modules.rs` - the fixture compiles and the type is
  reachable.

## Minimal test

```
cargo test -p cyclonedds-build
cargo test -p cyclonedds-test-suite --test nested_modules -- --test-threads=1
```

## Files

* Authored/changed: `cyclonedds-build/src/idl_parser.rs`, `cyclonedds-build/src/codegen.rs`,
  `cyclonedds-test-suite/Cargo.toml`, `cyclonedds-test-suite/build.rs` (new),
  `cyclonedds-test-suite/tests/idl/codegen/nested_modules.idl` (new),
  `cyclonedds-test-suite/tests/nested_modules.rs` (new).
* Planning: `planning/current_story.md` (new), `planning/backlog.md` (fixture-path update).
