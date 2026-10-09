// Verifies that the type name the Rust codegen emits matches the name the C idlc registers,
// and that the generated type round-trips over DDS.
//
// The expected name is transcribed from scripts/regen-typename-fixture.sh:
//   idlc 11.0.1, tests/idl/codegen/nested_modules.idl ->
//   .m_typename = "dds::hello_world::HelloWorldModel"

include!(concat!(env!("OUT_DIR"), "/gen/nested_modules.rs"));

use cyclonedds::*;
use cyclonedds_test_suite::{short_delay, unique_topic, wait_for};
use std::time::Duration;

/// The name the C `idlc` registers for `dds::hello_world::HelloWorldModel`.
const IDLC_TYPENAME: &str = "dds::hello_world::HelloWorldModel";

#[test]
fn type_name_matches_idlc() {
    assert_eq!(
        <dds::hello_world::HelloWorldModel as DdsType>::type_name(),
        IDLC_TYPENAME
    );
}

#[test]
fn generated_type_round_trips() {
    let participant = DomainParticipant::new(0).unwrap();
    let publisher = participant.create_publisher().unwrap();
    let subscriber = participant.create_subscriber().unwrap();
    let topic = participant
        .create_topic::<dds::hello_world::HelloWorldModel>(&unique_topic("typename"))
        .unwrap();
    let writer = publisher.create_writer(&topic).unwrap();
    let reader = subscriber.create_reader(&topic).unwrap();

    short_delay();
    writer
        .write(&dds::hello_world::HelloWorldModel {
            id: 7,
            message: "hi".to_string(),
        })
        .unwrap();

    assert!(wait_for(Duration::from_secs(2), || !reader
        .read()
        .unwrap()
        .is_empty()));

    let taken = reader.take().unwrap();
    assert!(!taken.is_empty());
    assert_eq!(taken[0].id, 7);
    assert_eq!(taken[0].message, "hi");
}
