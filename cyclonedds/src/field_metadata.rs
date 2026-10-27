//! Compile-time field metadata for generated DDS types.
//!
//! `#[derive(DdsType)]` and the other derive macros implement the native wire
//! contract. They deliberately do not carry a runtime field model: Rust has no
//! reflection, and the descriptor's ops stream does not preserve field names or
//! element types. [`DdsFieldMetadata`] is the opt-in bridge for tooling (such as
//! a topic inspector or a generic send form) that needs to enumerate fields and
//! convert between a typed sample and a [`DynamicValue`].
//!
//! `cyclonedds-build` emits an implementation for every type it generates. A
//! hand-written type can implement the trait too, but nothing derives it
//! automatically.

use crate::{DdsError, DdsResult, DynamicTypeSchema, DynamicValue};

/// Field-level metadata and typed ↔ dynamic conversion for a generated type.
///
/// The schema is cached by the generated implementation, so repeated calls are
/// cheap. Convert with [`DdsFieldMetadata::to_dynamic_value`] and
/// [`DdsFieldMetadata::from_dynamic_value`]; [`DdsFieldMetadata::get_field`] and
/// [`DdsFieldMetadata::set_field`] are convenience methods for a single field.
pub trait DdsFieldMetadata: Sized + 'static {
    /// The type's schema, cached for the lifetime of the program.
    fn field_schema() -> &'static DynamicTypeSchema;

    /// Convert a typed sample into its dynamic value tree.
    fn to_dynamic_value(&self) -> DynamicValue;

    /// Rebuild a typed sample from a dynamic value tree.
    ///
    /// Returns [`DdsError::BadParameter`] when the value does not have the
    /// shape of this type, when a required field is missing, or when a field
    /// has the wrong value kind.
    fn from_dynamic_value(value: &DynamicValue) -> DdsResult<Self>;

    /// The immediate field names of a struct, in declaration order.
    ///
    /// Non-struct types (enums, unions, bitmasks) have no named fields and
    /// return an empty vector.
    fn field_names() -> Vec<String> {
        match Self::field_schema() {
            DynamicTypeSchema::Struct { fields, .. } => {
                fields.iter().map(|field| field.name.clone()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Return one field's dynamic value.
    fn get_field(&self, name: &str) -> DdsResult<DynamicValue> {
        self.to_dynamic_value().field(name).cloned().ok_or_else(|| {
            DdsError::BadParameter(format!(
                "field '{name}' is not present in {}",
                std::any::type_name::<Self>()
            ))
        })
    }

    /// Replace one field by name, validating it against the field's schema.
    fn set_field(&mut self, name: &str, value: DynamicValue) -> DdsResult<()> {
        let mut tree = self.to_dynamic_value();
        tree.set_field(Self::field_schema(), name, value)?;
        *self = Self::from_dynamic_value(&tree)?;
        Ok(())
    }
}
