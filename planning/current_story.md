# Current story: optional-string-derive

This file is the detailed, living plan for the one active story. It is rewritten for each
story. The backlog in `backlog.md` holds all work and the per-story status.

## Story

Fix `#[derive(DdsType)]` mis-round-tripping `Option<String>`: for an optional unbounded string
the derive emits `OP_FLAG_OPT | OP_FLAG_EXT | TYPE_STR` and stores a pointer-to-`DdsString`,
but `idlc` emits `OP_FLAG_OPT | TYPE_STR` with the string inline (`char*`, null = absent).
Correct the op flags and the native representation; add a hand-written round-trip test, a
differential ops test, and re-enable `@optional string` in the codegen fixture.

- **Depends on:** none.
- **Minimal test:** a `cyclonedds-test-suite` round-trip of a hand-written struct with an
  `Option<String>` field; the differential ops test for optional string; and `optional.idl`
  with `@optional string` round-trips.

## Decisions

* **Root cause (confirmed).** `cyclonedds-src/.../tools/idlc/src/libidlc/libidlc__descriptor.c:1614-1615`:
  `opcode |= DDS_OP_FLAG_OPT | (idl_is_unbounded_xstring(type_spec) ? 0 : DDS_OP_FLAG_EXT)`.
  So an optional unbounded string is `OPT` without `EXT`; the member is an inline `char*` and a
  null pointer means absent. The derive adds `EXT` and stores a `*mut c_void` to a `DdsString`
  (`char**`), so the reader's `char*` is reinterpreted as a pointer on the Rust side.
* **Fix only unbounded direct strings (`String`).** Bounded strings (`DdsString`) keep
  `OPT | EXT` (matches `idlc`); they are out of scope.
* **Native representation** for an optional unbounded string becomes an inline
  `cyclonedds::DdsString` (`char*`), with `DdsString::null()` for `None`, replacing the
  `*mut c_void`-to-`DdsString` indirection. Both are 8 bytes, so the `Native` layout size is
  unchanged.
* **Differential fixture** added to `tests/idl/ops_reference.idl` to lock the exact op flags,
  following the repo's `ops_vs_idlc.rs` method.
* **Local verification only.** ASan is a CI-only job (nightly + Linux target); locally run the
  test/clippy/fmt gates and the unsafe inventory check. The derive's generated `quote!` code
  could change the `unsafe` count, so re-check the inventory.
* `idlc` is already available for regenerating the ops fixture (`scripts/regen-ops-fixtures.sh`
  builds it out of tree; `scripts/regen-typename-fixture.sh` locates an existing one).

## Deliverables

1. `cyclonedds-derive/src/lib.rs` - optional unbounded string op + native field/init/clone.
2. `cyclonedds-test-suite/tests/basic.rs` (or a new `optional_string.rs`) - hand-written
   `Option<String>` round-trip.
3. `cyclonedds-test-suite/tests/idl/ops_reference.idl` + `tests/ops_vs_idlc.rs` -
   optional-string ops differential.
4. `cyclonedds-test-suite/tests/idl/codegen/optional.idl` + `tests/optional.rs` - re-add
   `@optional string note` and round-trip it.

## Interfaces

No public API changes; the fix is in the derive's generated code.

### `cyclonedds-derive/src/lib.rs` (op emission, ~line 236)

Before:
```
OP_ADR | OP_FLAG_OPT | OP_FLAG_EXT | TYPE_STR,  offset
```
After:
```
OP_ADR | OP_FLAG_OPT | TYPE_STR,  offset
```

### `cyclonedds-derive/src/lib.rs` (native, ~line 555)

For `is_direct_string(inner_ty)`:

| | before | after |
|---|---|---|
| native field | `*mut c_void` | `cyclonedds::DdsString` |
| init (`Some`/`None`) | `arena.hold(DdsString::new(v)?) as *const DdsString as *mut c_void` / `null_mut()` | `DdsString::new(v)?` / `DdsString::null()` |
| clone-out | `Some((*(p as *const DdsString)).to_string_lossy())` / `None` | `Some(native.to_string_lossy())` / `None` (when `native.is_null()`) |

Other optional inner types (enum/primitive/composite) keep the existing `*mut c_void` handling.

## Pseudocode

### Op (`len_expr` for `option_inner` + `is_direct_string`)

```
} else if is_direct_string(&inner_ty) {
    main_ops_parts.push(quote! {
        __ops.push(cyclonedds::OP_ADR | cyclonedds::OP_FLAG_OPT | cyclonedds::TYPE_STR);
        __ops.push(#offset_expr);
    });
    quote! { 2u32 }
}
```

### Native (`if let Some(inner_ty) = option_inner.as_ref()`)

```
if is_direct_string(inner_ty) {
    native_fields.push(quote! { pub #field_name: cyclonedds::DdsString, });
    native_init_fields.push(quote! {
        #field_name: match &self.#field_name {
            Some(value) => cyclonedds::DdsString::new(value)?,
            None => cyclonedds::DdsString::null(),
        },
    });
    clone_fields.pop();
    clone_fields.push(quote! {
        #field_name: if __raw.#field_name.is_null() {
            None
        } else {
            Some(__raw.#field_name.to_string_lossy())
        },
    });
} else {
    // existing enum / primitive / composite handling (unchanged)
}
```

### Differential fixture (`ops_reference.idl`, inside `module ops_reference`)

```idl
struct OptStr {
  long h;
  @optional string s;
};
```

### Differential test (`ops_vs_idlc.rs`)

```
// idlc:
//   ADR|4BY|SGN, offsetof(h)
//   ADR|OPT|STR, offsetof(s)
//   RTS
#[test]
fn ops_optional_string_matches_idlc() {
    type N = <OptStr as DdsType>::Native;
    assert_eq!(<OptStr as DdsType>::ops(), vec![
        ADR_I32,
        offset_of!(N, h) as u32,
        OP_ADR | OP_FLAG_OPT | TYPE_STR,
        offset_of!(N, s) as u32,
        OP_RTS,
    ]);
}
```

## Tests

* `cyclonedds-test-suite` hand-written:
  * `OptionalStringMessage { #[key] id: i32, note: Option<String> }`; write `Some("hi")` and
    read back `Some("hi")`; write `None` and read back `None`.
* `tests/idl/codegen/optional.idl`: add `@optional string note;`; `tests/optional.rs`
  round-trips `Some`/`None` for `note`.
* `tests/ops_vs_idlc.rs`: `ops_optional_string_matches_idlc` as above.
* Re-run the existing `optional`, `cross_module`, `basic` suites (no regression).

## Minimal test

```
cargo test -p cyclonedds-test-suite --test basic --test optional --test ops_vs_idlc -- --test-threads=1
cargo test -p cyclonedds-derive
cargo clippy -p cyclonedds-derive -p cyclonedds-test-suite --all-targets -- -D warnings -A missing_docs
bash scripts/ci/check-unsafe-contracts.sh
```

(ASan is a CI-only job; not run locally.)

## Files

* Authored/changed: `cyclonedds-derive/src/lib.rs`,
  `cyclonedds-test-suite/tests/basic.rs` (or new `optional_string.rs`),
  `cyclonedds-test-suite/tests/idl/ops_reference.idl`,
  `cyclonedds-test-suite/tests/ops_vs_idlc.rs`,
  `cyclonedds-test-suite/tests/idl/codegen/optional.idl`,
  `cyclonedds-test-suite/tests/optional.rs`.
* Planning: `planning/current_story.md`, `planning/backlog.md`.
