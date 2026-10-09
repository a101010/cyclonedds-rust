# Current story: includes

This file is the detailed, living plan for the one active story. It is rewritten for each
story. The backlog in `backlog.md` holds all work and the per-story status.

## Story

Add `include_dirs: Vec<PathBuf>` to `CompileOptions` and `--include-dir` to
`cyclonedds-idlc`/`cargo-cyclonedds`; add `src/preprocessor.rs` to expand `#include "..."`
/ `#include <...>` and `import`, search `include_dirs` then the including file's directory,
detect cycles, and feed the combined source to the parser; teach the tokenizer to accept `#`.
Add include fixtures under `cyclonedds-test-suite/tests/idl/`.

- **Depends on:** none.
- **Minimal test:** `a.idl` includes `b.idl`; compiling `a.idl` with `--include-dir` yields
  types from both files; a cyclic include returns `Err`.

## Decisions

* **Angle-bracket includes are supported.** OMG IDL 4.1 §7.2.5 says IDL is preprocessed per
  ISO/IEC 14882:2003 (C++03), whose `#include` has both forms; CycloneDDS `idlc` delegates to
  the `mcpp` C preprocessor, which supports both. So both `"..."` and `<...>` are implemented.
* **Search order (conventional C).** Quoted `"file"`: the including file's directory first,
  then `include_dirs` in order. Angle `<file>`: `include_dirs` only (the including file's
  directory is not searched).
* **`import`.** File-form `import "file";` / `import <file>;` is inlined exactly like
  `#include`. Module-form `import Foo::Bar;` is deferred and left to the parser's existing
  skip.
* **Macros are not implemented.** The spec references the full C++ preprocessor, so IDL also
  permits `#define`/macros. This simplified preprocessor handles `#include`/`import` and drops
  other `#` lines; it does not expand macros. This matches the built-in parser's simplified
  stance.
* **Cycle detection, no dedup.** A stack of canonicalized paths currently being expanded; a
  repeat on the stack is an error naming the chain. Diamond includes (the same file reached by
  two paths) are not deduplicated; the parser/codegen reports the resulting duplicate
  definitions.
* **`rerun-if-changed`.** `preprocess` returns every file read, so `compile_idl_with_options`
  emits `cargo:rerun-if-changed` for each (today only the root IDL is emitted).
* **Tokenizer.** A `#` line is skipped to end-of-line. Includes are expanded before tokenizing,
  so this only mops up leftovers (`#pragma`, etc.).
* **Fixture placement.** The end-to-end include fixture lives under `tests/idl/codegen/` (so
  `build.rs` generates and compiles it); cycle/`import`/search cases are `tempfile`-based unit
  tests in `cyclonedds-build` (a deliberately broken fixture pair is not committed); the
  `--include-dir` end-to-end lives in a `cyclonedds-idlc` CLI test.
* `include_dirs` defaults to empty, so existing behavior is unchanged.

## Deliverables

1. `cyclonedds-build/src/preprocessor.rs` (new) - expansion, search, cycle detection, tests.
2. `cyclonedds-build/src/lib.rs` - `include_dirs` field; run the preprocessor and emit
   per-file `rerun-if-changed`.
3. `cyclonedds-build/src/idl_parser.rs` - tokenizer accepts `#`.
4. `cyclonedds-idlc/src/main.rs`, `cargo-cyclonedds/src/main.rs` - `--include-dir`.
5. `cyclonedds-test-suite/tests/idl/codegen/includes.idl`, `includes_types.idl` (new) and
   `cyclonedds-test-suite/tests/includes.rs` (new).
6. `cyclonedds-idlc/tests/cli.rs` (new) + `tempfile` dev-dependency.

## Interfaces

### `cyclonedds-build/src/preprocessor.rs` (new)

```
/// The result of expanding a file's `#include`/`import` directives.
pub struct Preprocessed {
    /// Combined source with includes inlined and directives removed.
    pub source: String,
    /// Every file read (root first, then includes in read order), for rerun-if-changed.
    pub files: Vec<PathBuf>,
}

/// Expand `#include`/`import` directives in `root`, resolving against `include_dirs`.
pub fn preprocess(root: &Path, include_dirs: &[PathBuf]) -> Result<Preprocessed, String>;

// private
struct IncludeSpec { name: String, quoted: bool }
fn parse_include_line(line: &str) -> Option<IncludeSpec>;   // `#include "x"` / `#include <x>`
fn parse_import_line(line: &str) -> Option<IncludeSpec>;    // `import "x";` / `import <x>;`
fn resolve(spec: &IncludeSpec, including_dir: &Path, include_dirs: &[PathBuf])
    -> Result<PathBuf, String>;
fn expand(path: &Path, include_dirs: &[PathBuf], stack: &mut Vec<PathBuf>,
          files: &mut Vec<PathBuf>) -> Result<String, String>;
```

### `cyclonedds-build/src/lib.rs`

```
pub struct CompileOptions {
    ...
    pub include_dirs: Vec<PathBuf>,   // NEW, default empty
}
impl Default for CompileOptions { ... include_dirs: Vec::new() ... }

pub fn compile_idl_with_options(idl_path: &Path, options: &CompileOptions) -> Result<()>;
```

`compile_idl`, `compile_idl_files`, `validate_idl`, `parse_and_generate`,
`compile_with_idlc_or_fallback` signatures unchanged (they already take/forward `options`).

### `cyclonedds-build/src/idl_parser.rs`

No signature changes; `tokenize` gains a `'#'` arm.

### `cyclonedds-idlc/src/main.rs`, `cargo-cyclonedds/src/main.rs`

```
/// Add a directory to the #include/import search path (repeatable).
#[arg(long = "include-dir", value_name = "DIR")]
include_dirs: Vec<PathBuf>,
```

### `cyclonedds-test-suite/tests/includes.rs` (new)

```
include!(concat!(env!("OUT_DIR"), "/gen/includes.rs"));
#[test] fn included_types_are_generated();
```

## Pseudocode

### `preprocessor.rs`

```
preprocess(root, include_dirs):
    files = []
    stack = []
    source = expand(root, include_dirs, stack, files)
    return { source, files }

expand(path, include_dirs, stack, files):
    canonical = canonicalize(path) or path
    if canonical in stack:
        return Err("include cycle: " + join(" -> ", stack + [canonical]))
    stack.push(canonical)
    files.push(path)
    text = read_to_string(path)?
    including_dir = path.parent()
    out = ""
    for line in text.lines():
        trimmed = trim_start(line)
        spec = parse_include_line(trimmed) or parse_import_line(trimmed)
        if spec is Some:
            resolved = resolve(spec, including_dir, include_dirs)?
            out += expand(resolved, include_dirs, stack, files) + "\n"
        elif trimmed starts with "#":
            out += "\n"                 // drop other directives (#pragma, ...)
        else:
            out += line + "\n"
    stack.pop()
    return out

resolve(spec, including_dir, include_dirs):
    if spec.quoted:
        candidate = including_dir / spec.name
        if exists(candidate): return candidate
    for dir in include_dirs:
        candidate = dir / spec.name
        if exists(candidate): return candidate
    return Err("included file not found: " + spec.name)

parse_include_line(line):
    if not line starts with "#include": return None
    rest = line after "#include", trimmed
    if rest starts with '"': return Some({ name: up to next '"', quoted: true })
    if rest starts with '<': return Some({ name: up to next '>', quoted: false })
    return None

parse_import_line(line):
    if not line starts with "import": return None
    rest = line after "import", trimmed
    // same quoted/angle parsing, ignoring a trailing ';'
    ...
```

### `idl_parser.rs` tokenizer (new arm, before `_ => Err`)

```
'#' => {
    chars.next()
    while peek() is Some(c) and c != '\n': chars.next()
}
```

### `lib.rs`

```
compile_idl_with_options(idl_path, options):
    if !idl_path.exists(): bail("IDL file not found: ...")
    pre = preprocessor::preprocess(idl_path, &options.include_dirs)
              .map_err(|e| anyhow!("IDL preprocessing failed: {e}"))?
    module_name = options.module_name or stem(idl_path)
    rust_code = if options.try_idlc:
                    compile_with_idlc_or_fallback(&pre.source, &module_name, options)?
                else:
                    parse_and_generate(&pre.source, &module_name, options)?
    ... write output_dir/module_name.rs ...
    if OUT_DIR is set:
        for f in &pre.files: println!("cargo:rerun-if-changed={}", f.display())
```

### `cyclonedds-idlc/tests/cli.rs`

```
test include_dir_flag_compiles_an_idl_with_includes():
    dir = tempdir()
    write dir/main.idl  = "#include \"dep.idl\"\nstruct Main { long a; };"
    write dir/inc/dep.idl = "struct Dep { long b; };"
    run CARGO_BIN_EXE_cyclonedds-idlc
        --input dir/main.idl --output-dir dir/out --include-dir dir/inc --no-idlc
    assert status.success()
    generated = read dir/out/main.rs
    assert generated contains "struct Main" and "struct Dep"
```

## Fixtures

`cyclonedds-test-suite/tests/idl/codegen/includes_types.idl`:

```idl
module shared {
  struct SharedType {
    long value;
  };
};
```

`cyclonedds-test-suite/tests/idl/codegen/includes.idl`:

```idl
#include "includes_types.idl"

module dds {
  module hello_world {
    struct Included {
      @key long id;
      string message;
    };
  };
};
```

## Tests

* `preprocessor.rs` (unit, `tempfile`):
  * `includes_same_directory` - `a.idl` `#include "b.idl"`, no include dirs; both bodies present.
  * `includes_via_include_dir` - dependency only in `include_dirs` (angle and quoted).
  * `quoted_prefers_including_dir` - same name in both places resolves to the sibling.
  * `cycle_is_error` - `a` -> `b` -> `a` returns `Err` naming the cycle.
  * `import_is_inlined` - `import "b.idl";` inlines `b`.
  * `files_lists_every_file_read` - `files` contains root and dependency.
* `lib.rs` (unit): `test_compile_with_include_dir` - write `a.idl`/`b.idl` in a temp dir,
  compile with `include_dirs`, assert output has both structs.
* `cyclonedds-test-suite/tests/includes.rs`: `shared::SharedType` and
  `dds::hello_world::Included` exist with the expected `type_name()`s.
* `cyclonedds-idlc/tests/cli.rs`: the CLI smoke test above.
* `cargo-cyclonedds`: covered by compilation of the new flag.

## Minimal test

```
cargo test -p cyclonedds-build
cargo test -p cyclonedds-idlc --test cli
cargo test -p cyclonedds-test-suite --test includes -- --test-threads=1
cargo clippy -p cyclonedds-build -p cyclonedds-idlc -p cargo-cyclonedds --all-targets -- -D warnings -A missing_docs
```

## Result

Done and verified. `cyclonedds-build/src/preprocessor.rs` expands `#include "..."` / `#include
<...>` and file-form `import`; quoted includes resolve against the including file's directory
then `include_dirs`, angle includes against `include_dirs` only; cycles are detected via a
canonicalized stack. `CompileOptions` gained `include_dirs`; `compile_idl_with_options`
preprocesses before parsing and emits `cargo:rerun-if-changed` for every file read. The
tokenizer now skips `#` lines. `--include-dir` was added to `cyclonedds-idlc` and
`cargo-cyclonedds`. Fixtures `tests/idl/codegen/includes.idl` + `includes_types.idl` compile
through `build.rs` and `tests/includes.rs` asserts both `shared::SharedType` and
`dds::hello_world::Included`.

`cargo test -p cyclonedds-build` (28 tests), `cargo test -p cyclonedds-idlc --test cli`, and
`cargo test -p cyclonedds-test-suite --test includes --test nested_modules --test
typename_vs_idlc -- --test-threads=1` pass; `cargo fmt --all -- --check` and clippy are clean;
unsafe inventory PASS.

## Files

* Authored/changed: `cyclonedds-build/src/preprocessor.rs` (new), `cyclonedds-build/src/lib.rs`,
  `cyclonedds-build/src/idl_parser.rs`, `cyclonedds-idlc/src/main.rs`,
  `cargo-cyclonedds/src/main.rs`, `cyclonedds-idlc/Cargo.toml` (`tempfile` dev-dep),
  `cyclonedds-idlc/tests/cli.rs` (new),
  `cyclonedds-test-suite/tests/idl/codegen/includes.idl` + `includes_types.idl` (new),
  `cyclonedds-test-suite/tests/includes.rs` (new).
* Planning: `planning/current_story.md`, `planning/backlog.md`.
