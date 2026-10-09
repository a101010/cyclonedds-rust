// Verifies that `cyclonedds-build` generates nested Rust modules and that the result
// compiles. The IDL is compiled by `build.rs` from `tests/idl/codegen/nested_modules.idl`
// and included at the crate root so the generated `#![allow(...)]` stays a crate attribute.

include!(concat!(env!("OUT_DIR"), "/gen/nested_modules.rs"));

use cyclonedds::DdsType;

#[test]
fn nested_modules_compile_and_expose_types() {
    assert_eq!(
        <dds::hello_world::HelloWorldModel as DdsType>::type_name(),
        "dds::hello_world::HelloWorldModel"
    );
}
