// Verifies that scoped references generate resolvable Rust paths: `shapes::Line` holds
// `super::geometry::Point` fields, so this test only compiles if the resolution is right.

include!(concat!(env!("OUT_DIR"), "/gen/cross_module.rs"));

use cyclonedds::DdsType;

#[test]
fn cross_module_references_compile_and_name_correctly() {
    assert_eq!(<geometry::Point as DdsType>::type_name(), "geometry::Point");
    assert_eq!(<shapes::Line as DdsType>::type_name(), "shapes::Line");
}
