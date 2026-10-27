// Verifies generated `DdsFieldMetadata` impls: schema enumeration, typed ↔
// dynamic conversion, and field get/set.

include!(concat!(env!("OUT_DIR"), "/gen/field_metadata.rs"));

use cyclonedds::{DdsFieldMetadata, DynamicPrimitiveKind, DynamicTypeSchema, DynamicValue};

fn sample() -> field_meta::MetadataTopic {
    field_meta::MetadataTopic {
        id: 7,
        active: true,
        raw: 255,
        letter: b'x',
        code: 42,
        count: -3,
        name: "name".to_string(),
        tag: DdsString::new("tag").unwrap(),
        inner: field_meta::Inner {
            x: 1.5,
            label: "inner".to_string(),
        },
        alias: field_meta::Inner {
            x: 2.5,
            label: "alias".to_string(),
        },
        numbers: DdsSequence::from_slice(&[1, 2, 3]).unwrap(),
        bounded: DdsBoundedSequence::<i32, 4>::from_slice(&[4, 5]).unwrap(),
        fixed: [6, 7, 8],
        note: Some("note".to_string()),
        maybe: Some(field_meta::Inner {
            x: 3.5,
            label: "maybe".to_string(),
        }),
    }
}

#[test]
fn schema_enumerates_fields() {
    let schema = <field_meta::MetadataTopic as DdsFieldMetadata>::field_schema();
    let fields = match schema {
        DynamicTypeSchema::Struct { name, fields, .. } => {
            assert_eq!(name, "field_meta::MetadataTopic");
            fields
        }
        other => panic!("unexpected schema: {other:?}"),
    };

    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "id", "active", "raw", "letter", "code", "count", "name", "tag", "inner", "alias",
            "numbers", "bounded", "fixed", "note", "maybe",
        ]
    );

    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field.name == name)
            .unwrap_or_else(|| panic!("missing field {name}"))
    };

    assert!(field("id").key);
    assert!(!field("count").optional);
    assert!(field("note").optional);
    assert!(field("maybe").optional);

    assert_eq!(
        field("count").value,
        DynamicTypeSchema::Primitive(DynamicPrimitiveKind::Int32)
    );
    assert_eq!(
        field("raw").value,
        DynamicTypeSchema::Primitive(DynamicPrimitiveKind::UInt8)
    );
    assert_eq!(
        field("letter").value,
        DynamicTypeSchema::Primitive(DynamicPrimitiveKind::Char8)
    );
    assert_eq!(
        field("tag").value,
        DynamicTypeSchema::String { bound: Some(16) }
    );
    assert_eq!(
        field("note").value,
        DynamicTypeSchema::String { bound: None }
    );

    match &field("numbers").value {
        DynamicTypeSchema::Sequence { bound, element, .. } => {
            assert_eq!(*bound, None);
            assert_eq!(
                **element,
                DynamicTypeSchema::Primitive(DynamicPrimitiveKind::Int32)
            );
        }
        other => panic!("unexpected numbers schema: {other:?}"),
    }
    match &field("bounded").value {
        DynamicTypeSchema::Sequence { bound, element, .. } => {
            assert_eq!(*bound, Some(4));
            assert_eq!(
                **element,
                DynamicTypeSchema::Primitive(DynamicPrimitiveKind::Int32)
            );
        }
        other => panic!("unexpected bounded schema: {other:?}"),
    }
    match &field("fixed").value {
        DynamicTypeSchema::Array {
            bounds, element, ..
        } => {
            assert_eq!(*bounds, vec![3]);
            assert_eq!(
                **element,
                DynamicTypeSchema::Primitive(DynamicPrimitiveKind::Int32)
            );
        }
        other => panic!("unexpected fixed schema: {other:?}"),
    }

    let inner_schema = <field_meta::Inner as DdsFieldMetadata>::field_schema().clone();
    assert_eq!(field("inner").value, inner_schema);
    assert_eq!(field("alias").value, inner_schema);

    assert_eq!(
        <field_meta::MetadataTopic as DdsFieldMetadata>::field_names(),
        names
    );
}

#[test]
fn value_round_trips_through_dynamic() {
    let original = sample();
    let value = original.to_dynamic_value();

    match &value {
        DynamicValue::Struct(fields) => {
            assert_eq!(
                fields.get("name"),
                Some(&DynamicValue::String("name".to_string()))
            );
            assert_eq!(
                fields.get("note"),
                Some(&DynamicValue::String("note".to_string()))
            );
            assert!(matches!(
                fields.get("numbers"),
                Some(DynamicValue::Sequence(values)) if values.len() == 3
            ));
            assert!(matches!(
                fields.get("fixed"),
                Some(DynamicValue::Array(values)) if values.len() == 3
            ));
        }
        other => panic!("unexpected value: {other:?}"),
    }

    let restored = field_meta::MetadataTopic::from_dynamic_value(&value).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn field_get_and_set() {
    let mut sample = sample();

    assert_eq!(sample.get_field("count").unwrap(), DynamicValue::Int32(-3));
    sample.set_field("count", DynamicValue::Int32(99)).unwrap();
    assert_eq!(sample.count, 99);

    sample.set_field("note", DynamicValue::Null).unwrap();
    assert_eq!(sample.note, None);
    sample
        .set_field("note", DynamicValue::String("new".to_string()))
        .unwrap();
    assert_eq!(sample.note.as_deref(), Some("new"));

    let inner = field_meta::Inner {
        x: 2.5,
        label: "changed".to_string(),
    };
    sample
        .set_field(
            "inner",
            <field_meta::Inner as DdsFieldMetadata>::to_dynamic_value(&inner),
        )
        .unwrap();
    assert_eq!(sample.inner, inner);

    assert!(sample
        .set_field("count", DynamicValue::String("bad".to_string()))
        .is_err());
}

#[test]
fn missing_required_field_is_an_error() {
    let sample = sample();
    let mut value = sample.to_dynamic_value();
    if let DynamicValue::Struct(fields) = &mut value {
        fields.remove("id");
    }
    assert!(field_meta::MetadataTopic::from_dynamic_value(&value).is_err());
}

#[test]
fn enum_metadata_round_trips() {
    let schema = <field_meta::Color as DdsFieldMetadata>::field_schema();
    match schema {
        DynamicTypeSchema::Enum { literals, .. } => {
            assert_eq!(literals.len(), 3);
            assert_eq!(literals[1].name, "GREEN");
            assert_eq!(literals[1].value, 1);
        }
        other => panic!("unexpected enum schema: {other:?}"),
    }

    let value =
        <field_meta::Color as DdsFieldMetadata>::to_dynamic_value(&field_meta::Color::GREEN);
    assert_eq!(value, DynamicValue::Enum { value: 1 });
    assert_eq!(
        field_meta::Color::from_dynamic_value(&value).unwrap(),
        field_meta::Color::GREEN
    );
    assert!(field_meta::Color::from_dynamic_value(&DynamicValue::Enum { value: 99 }).is_err());
}

#[test]
fn bitmask_metadata_round_trips() {
    let mut flags = field_meta::Flags {
        Ready: true,
        Active: false,
    };
    assert!(flags.Ready());
    assert!(!flags.Active());
    flags.set_Active(true);
    assert!(flags.Active());
    let value = <field_meta::Flags as DdsFieldMetadata>::to_dynamic_value(&flags);
    assert_eq!(value, DynamicValue::Bitmask(3));
    assert_eq!(
        <field_meta::Flags as DdsFieldMetadata>::from_dynamic_value(&value)
            .unwrap()
            .to_bits(),
        3
    );

    match <field_meta::Flags as DdsFieldMetadata>::field_schema() {
        DynamicTypeSchema::Bitmask {
            fields, bit_bound, ..
        } => {
            assert_eq!(*bit_bound, Some(8));
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "Ready");
            assert_eq!(fields[0].position, 0);
            assert_eq!(fields[1].name, "Active");
            assert_eq!(fields[1].position, 1);
        }
        other => panic!("unexpected bitmask schema: {other:?}"),
    }
}

#[test]
fn union_metadata_round_trips() {
    let number = field_meta::Choice::Number(5);
    let value = <field_meta::Choice as DdsFieldMetadata>::to_dynamic_value(&number);
    assert_eq!(value.union_field_name(), Some("number"));
    assert_eq!(value.union_discriminator(), Some(0));
    let restored = field_meta::Choice::from_dynamic_value(&value).unwrap();
    assert_eq!(
        <field_meta::Choice as DdsFieldMetadata>::to_dynamic_value(&restored),
        value
    );

    let other = field_meta::Choice::Fallback(9);
    let value = <field_meta::Choice as DdsFieldMetadata>::to_dynamic_value(&other);
    assert_eq!(value.union_field_name(), Some("fallback"));
    let restored = field_meta::Choice::from_dynamic_value(&value).unwrap();
    assert_eq!(
        <field_meta::Choice as DdsFieldMetadata>::to_dynamic_value(&restored),
        value
    );
}
