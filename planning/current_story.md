# Current story: literals-optional-failloud

This file is the detailed, living plan for the one active story. It is rewritten for each
story. The backlog in `backlog.md` holds all work and the per-story status.

## Story

Accept float and hex literals in the tokenizer; emit `Option<inner>` for `@optional` fields
and error on `@key @optional`; resolve scoped type references to correct Rust paths; return an
error naming unsupported constructs instead of silently skipping. Add an `@optional` fixture
and round-trip test to `cyclonedds-test-suite`.

- **Depends on:** nested-modules.
- **Minimal test:** `@optional long x;` generates `pub x: Option<i32>`; `@key @optional` and an
  unsupported construct return `Err`; a cross-module reference generates a resolvable path and
  the optional fixture round-trips.

## Decisions

* **Hex is currently broken, not just missing.** The tokenizer collects `0xFF` into the string
  `"0xFF"` and then `i64::from_str` rejects it (`Invalid integer`). Fix: parse hex with
  `i64::from_str_radix(digits, 16)`.
* **Floats** are tokenized as `FloatLit(f64)`; a decimal integer stays `IntLit(i64)`. A fraction
  is only consumed when `.` is followed by a digit (so `...`/`.` are unaffected); an exponent
  `e`/`E` (with optional sign) is consumed. Hex floats are not supported.
* **`@optional` needs no Rust attribute.** The derive detects `Option<T>` from the field type
  (`option_inner_type`), so codegen only wraps the inner type in `Option<...>`. `@optional` on
  an array/sequence/composite wraps the whole thing.
* **`@key @optional` errors in the parser** (in `parse_struct`, on the field's annotations),
  giving an early message rather than deferring to the derive's macro error.
* **Fail-loud scope:** `parse_definition` keeps `const`/`import`/`include`/`type`/`annotation`
  as explicitly skipped (they do not define topic types) and now returns
  `Err("unsupported construct: \`{kw}\`")` for any other unrecognized keyword (`interface`,
  `bitset`, `map`, `exception`, …) instead of silently skipping.
* **Scoped-reference resolution** builds a symbol table from `IdlFile::scoped_types()` and
  emits a path relative to the referring module using `super::` chains, so generated code works
  whether or not the file is `include!`d at the crate root. Unresolvable names fall back to the
  last segment (preserving today's behavior for types outside the tree).
* New fixtures `tests/idl/codegen/optional.idl` and `cross_module.idl` are compiled by
  `build.rs`; unit tests cover the tokenizer, the two error cases, and the codegen paths.

## Deliverables

1. `cyclonedds-build/src/idl_parser.rs` - `Token::FloatLit`; number-arm rewrite (hex + float);
   fail-loud `parse_definition`; `@key @optional` check; tests.
2. `cyclonedds-build/src/codegen.rs` - `SymbolTable`, `resolve_named`, `rust_path`;
   `type_ref_to_rust` gains `scope`/`symbols`; `@optional` -> `Option<...>`; tests.
3. `cyclonedds-test-suite/tests/idl/codegen/optional.idl`, `cross_module.idl` (new) +
   `tests/optional.rs`, `tests/cross_module.rs` (new).

## Interfaces

### `cyclonedds-build/src/idl_parser.rs`

```
enum Token { ..., IntLit(i64), FloatLit(f64), ... }   // FloatLit NEW

fn tokenize(input: &str) -> Result<Vec<Token>, String>;   // number arm rewritten

// Parser unchanged in signature; behavior:
//   parse_definition: unrecognized keyword -> Err("unsupported construct: `{kw}`")
//   parse_struct: field with both @key and @optional -> Err(...)
```

### `cyclonedds-build/src/codegen.rs`

```
struct TargetInfo { modules: Vec<String>, name: String }   // snake modules + type name
struct SymbolTable { types: HashMap<String, TargetInfo> }  // keyed by FQ IDL name
impl SymbolTable {
    fn from_file(file: &IdlFile) -> Self;
    fn lookup(&self, fq: &str) -> Option<&TargetInfo>;
}

fn resolve_named(ref_name: &str, scope: &[&str], symbols: &SymbolTable) -> String;
fn rust_path(scope: &[&str], target: &TargetInfo) -> String;

fn type_ref_to_rust(ty: &IdlTypeRef, scope: &[&str], symbols: &SymbolTable) -> String;

fn generate_definition(def, scope, symbols, options) -> String;
fn generate_module(m, scope, symbols, options) -> String;
fn generate_type(ty, scope, symbols, options) -> String;
fn generate_struct(s, scope, symbols, options) -> String;
fn generate_union(u, scope, symbols, options) -> String;
fn generate_typedef(td, scope, symbols, options) -> String;
```

`generate_enum`/`generate_bitmask` keep their current signatures.

## Pseudocode

### Tokenizer number arm

```
c if c.is_ascii_digit() || (c == '-' && next_is_digit()):
    negative = (c == '-')
    if negative: consume '-'
    if peek == '0' and peek2 in {'x','X'}:
        consume '0', 'x'
        digits = read while ascii_hexdigit
        consume suffixes (L/l/U/u)
        mag = i64::from_str_radix(digits, 16)?
        push IntLit(negative ? -mag : mag)
    else:
        num = negative ? "-" : ""
        read integer digits into num
        is_float = false
        if peek == '.' and char_after('.') is a digit:
            is_float = true; num += '.'; consume
            read fraction digits into num
        if peek in {'e','E'}:
            is_float = true; num += 'e'; consume
            if peek in {'+','-'}: num += consume
            read exponent digits into num
        if is_float:
            push FloatLit(num.parse::<f64>()?)
        else:
            consume suffixes (L/l/U/u)
            push IntLit(num.parse::<i64>()?)
```

### `parse_definition` (fail-loud)

```
match peek():
    Ident("module")   -> Module(parse_module)
    Ident("struct")   -> Type(parse_struct)
    Ident("enum")     -> Type(parse_enum)
    Ident("union")    -> Type(parse_union)
    Ident("bitmask")  -> Type(parse_bitmask)
    Ident("typedef")  -> parse_typedef
    Ident("const"|"import"|"include"|"type"|"annotation") -> skip_to_semi(); None
    Ident(other)      -> Err("unsupported construct: `{other}`")
    _                 -> advance(); None
```

### `parse_struct` (`@key @optional`)

```
field_annotations = parse_annotations()
(name, ty) = parse_field()
if field_annotations has "key" and has "optional":
    return Err("field `{name}`: @optional is not supported on @key fields")
fields.push(...)
```

### Codegen scoped resolution

```
SymbolTable::from_file(file):
    for st in file.scoped_types():
        modules = st.scope.map(to_snake_case)
        insert[st.qualified_name()] = { modules, name: st.ty.name() }

resolve_named(ref_name, scope, symbols):
    absolute = ref_name starts with "::"
    clean = ref_name.trim_start_matches("::")
    candidates =
        if absolute: [clean]
        else: for i in (0..=scope.len()).rev():
                  if i == 0: clean
                  else: scope[..i].join("::") + "::" + clean
    for cand in candidates:
        if target = symbols.lookup(cand): return rust_path(scope, target)
    return last_segment(clean)                       // fallback

rust_path(scope, target):
    common = longest prefix where to_snake_case(scope[i]) == target.modules[i]
    parts = ["super"] * (scope.len() - common)
    parts += target.modules[common..]
    parts += [target.name]
    return parts.join("::")

type_ref_to_rust(Named(name), scope, symbols):
    return resolve_named(name, scope, symbols)
// other variants recurse with scope/symbols

generate_struct(s, scope, symbols, options):
    ... derive / dds_typename ...
    for field:
        if field has "key": emit "#[key]"
        inner = type_ref_to_rust(field.ty, scope, symbols)
        ty = if field has "optional": "Option<" + inner + ">" else inner
        emit "pub {name}: {ty}"
```

## Fixtures

`cyclonedds-test-suite/tests/idl/codegen/optional.idl`:

```idl
module dds {
  module hello_world {
    struct OptionalModel {
      @key long id;
      @optional long count;
      @optional string note;
    };
  };
};
```

`cyclonedds-test-suite/tests/idl/codegen/cross_module.idl`:

```idl
module geometry {
  struct Point {
    double x;
    double y;
  };
};

module shapes {
  struct Line {
    geometry::Point start;
    geometry::Point end;
  };
};
```

## Tests

* `idl_parser.rs` (unit):
  * `test_tokenize_hex_and_float` - `tokenize("0xFF 3.14 1e10 -2.5")` yields `IntLit(255)`,
    `FloatLit(3.14)`, `FloatLit(1e10)`, `FloatLit(-2.5)`.
  * `test_parse_const_literals` - IDL with `const long M = 0xFF; const double PI = 3.14159;`
    parses without error.
  * `test_unsupported_construct_errors` - `interface Foo { void op(); };` returns `Err` naming
    the construct.
  * `test_key_optional_errors` - `@key @optional long x;` returns `Err`.
* `codegen.rs` (unit):
  * `test_generate_optional_field` - `@optional long x;` emits `pub x: Option<i32>`;
    `@optional string s;` emits `pub s: Option<String>`.
  * `test_generate_same_module_reference` - `module m { struct A {...}; struct B { A a; }; };`
    emits `pub a: A`.
  * `test_generate_cross_module_reference` - top-level `Line { geometry::Point start; }` emits
    `geometry::Point`; a `shapes`-nested `Line` emits `super::geometry::Point`.
* `cyclonedds-test-suite/tests/optional.rs` - includes `$OUT_DIR/gen/optional.rs`; round-trips
  `Some`/`None` for `count` and `note` over DDS.
* `cyclonedds-test-suite/tests/cross_module.rs` - includes `$OUT_DIR/gen/cross_module.rs`;
  asserts `geometry::Point` and `shapes::Line` exist with FQ `type_name()`s (proves the
  generated paths compile).

## Minimal test

```
cargo test -p cyclonedds-build
cargo test -p cyclonedds-test-suite --test optional --test cross_module -- --test-threads=1
cargo clippy -p cyclonedds-build --all-targets -- -D warnings -A missing_docs
```

## Files

* Authored/changed: `cyclonedds-build/src/idl_parser.rs`, `cyclonedds-build/src/codegen.rs`,
  `cyclonedds-test-suite/tests/idl/codegen/optional.idl` + `cross_module.idl` (new),
  `cyclonedds-test-suite/tests/optional.rs` + `cross_module.rs` (new).
* Planning: `planning/current_story.md`, `planning/backlog.md`.
