//! The shape of one collection.

use std::collections::BTreeSet;

use crate::FieldName;

/// The fields a collection exposes.
///
/// Deliberately does not model types. The Data API passes JSON values through
/// to the backend, which is the component that actually knows its own type
/// system; duplicating a type lattice here would mean maintaining a second,
/// less accurate one. What we do need is the *field set*, so that a request
/// naming a field that does not exist fails cleanly instead of producing a
/// backend error whose text varies by vendor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionSchema {
    fields: BTreeSet<FieldName>,
}

impl CollectionSchema {
    /// Builds a collection schema from its field names.
    #[must_use]
    pub fn new(fields: impl IntoIterator<Item = FieldName>) -> Self {
        Self {
            fields: fields.into_iter().collect(),
        }
    }

    /// Whether the collection has this field.
    #[must_use]
    pub fn has_field(&self, field: &FieldName) -> bool {
        self.fields.contains(field)
    }

    /// The collection's fields, in name order.
    pub fn fields(&self) -> impl Iterator<Item = &FieldName> {
        self.fields.iter()
    }

    /// How many fields the collection has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Whether the collection exposes no fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }
}
