// Verifies that `cyclonedds-build` expands `#include` and generates types from both files.
// The IDL is compiled by `build.rs` from `tests/idl/codegen/includes.idl`, which includes
// `tests/idl/codegen/includes_types.idl`.

include!(concat!(env!("OUT_DIR"), "/gen/includes.rs"));

use cyclonedds::DdsType;

#[test]
fn included_types_are_generated() {
    assert_eq!(
        <shared::SharedType as DdsType>::type_name(),
        "shared::SharedType"
    );
    assert_eq!(
        <dds::hello_world::Included as DdsType>::type_name(),
        "dds::hello_world::Included"
    );
}
