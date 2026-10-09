// Verifies that `@optional` members generate `Option<...>` and round-trip over DDS,
// including the absent (`None`) case.

include!(concat!(env!("OUT_DIR"), "/gen/optional.rs"));

use cyclonedds::*;
use cyclonedds_test_suite::{short_delay, unique_topic, wait_for};
use std::time::Duration;

#[test]
fn optional_fields_round_trip() {
    let participant = DomainParticipant::new(0).unwrap();
    let publisher = participant.create_publisher().unwrap();
    let subscriber = participant.create_subscriber().unwrap();
    let topic = participant
        .create_topic::<dds::hello_world::OptionalModel>(&unique_topic("optional"))
        .unwrap();
    let writer = publisher.create_writer(&topic).unwrap();
    let reader = subscriber.create_reader(&topic).unwrap();

    short_delay();

    writer
        .write(&dds::hello_world::OptionalModel {
            id: 1,
            count: Some(7),
            ratio: Some(2.5),
        })
        .unwrap();
    assert!(wait_for(Duration::from_secs(2), || !reader
        .read()
        .unwrap()
        .is_empty()));
    let taken = reader.take().unwrap();
    assert!(!taken.is_empty());
    assert_eq!(taken[0].id, 1);
    assert_eq!(taken[0].count, Some(7));
    assert_eq!(taken[0].ratio, Some(2.5));

    writer
        .write(&dds::hello_world::OptionalModel {
            id: 2,
            count: None,
            ratio: None,
        })
        .unwrap();
    assert!(wait_for(Duration::from_secs(2), || reader
        .read()
        .unwrap()
        .iter()
        .any(|s| s.id == 2)));
    let taken = reader.take().unwrap();
    let sample = taken.iter().find(|s| s.id == 2).expect("id=2 sample");
    assert_eq!(sample.count, None);
    assert_eq!(sample.ratio, None);
}
