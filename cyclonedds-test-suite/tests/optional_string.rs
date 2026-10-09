// Regression for `optional-string-derive`: `#[derive(DdsType)]` must round-trip an
// `Option<String>` field. `idlc` emits OPT without EXT for optional unbounded strings
// (the member is an inline `char*`; null means absent).

use cyclonedds::*;
use cyclonedds_test_suite::{short_delay, unique_topic, wait_for};
use std::time::Duration;

#[repr(C)]
#[derive(Debug, Clone, PartialEq, DdsTypeDerive)]
struct OptionalStringMessage {
    #[key]
    id: i32,
    note: Option<String>,
}

#[test]
fn optional_string_round_trips() {
    let participant = DomainParticipant::new(0).unwrap();
    let publisher = participant.create_publisher().unwrap();
    let subscriber = participant.create_subscriber().unwrap();
    let topic = participant
        .create_topic::<OptionalStringMessage>(&unique_topic("optional_string"))
        .unwrap();
    let writer = publisher.create_writer(&topic).unwrap();
    let reader = subscriber.create_reader(&topic).unwrap();

    short_delay();

    writer
        .write(&OptionalStringMessage {
            id: 1,
            note: Some("hi".to_string()),
        })
        .unwrap();
    assert!(wait_for(Duration::from_secs(2), || !reader
        .read()
        .unwrap()
        .is_empty()));
    let taken = reader.take().unwrap();
    assert!(!taken.is_empty());
    assert_eq!(taken[0].id, 1);
    assert_eq!(taken[0].note.as_deref(), Some("hi"));

    writer
        .write(&OptionalStringMessage { id: 2, note: None })
        .unwrap();
    assert!(wait_for(Duration::from_secs(2), || reader
        .read()
        .unwrap()
        .iter()
        .any(|s| s.id == 2)));
    let taken = reader.take().unwrap();
    let sample = taken.iter().find(|s| s.id == 2).expect("id=2 sample");
    assert_eq!(sample.note, None);
}
