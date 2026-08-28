//! NDC type references.

/// A type reference in a connector's schema.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcType {
    /// A named type — a scalar or an object type.
    Named {
        /// The type's name.
        name: String,
    },
    /// A nullable wrapper.
    Nullable {
        /// What it wraps.
        underlying_type: Box<NdcType>,
    },
    /// An array.
    Array {
        /// The element type.
        element_type: Box<NdcType>,
    },
    /// A predicate type, used for argument positions. Not a scalar.
    Predicate {
        /// The object type the predicate applies to.
        object_type_name: String,
    },
}

impl NdcType {
    /// The underlying named type, unwrapping nullability only.
    ///
    /// A column declared `nullable(named "text")` still compares with `text`'s
    /// operators — nullability changes whether a value is *present*, not what
    /// comparing one means — so that wrapper comes off before the operator
    /// lookup.
    ///
    /// # An array is deliberately not unwrapped
    ///
    /// NDC declares comparison operators per **scalar type**
    /// (`schema/scalar-types.md`), and an array is not one. Filtering an array
    /// has its own mechanism: an `array_comparison` expression, gated on the
    /// `query.nested_fields.filter_by.nested_arrays` capability with `contains`
    /// and `is_empty` sub-capabilities (`queries/filtering.md`). This crate
    /// neither reads that capability nor emits that expression.
    ///
    /// Returning the element's name here would have put the *element* scalar's
    /// `_eq` on the wire against the whole array — and on a document store,
    /// equality against an array conventionally means *contains*, which is
    /// strictly wider than what the caller asked for. That is the one thing
    /// [`crate::wire`]'s policy forbids, and it fails in the direction that
    /// returns rows rather than an error.
    ///
    /// So an array column has no scalar type, falls out of the operator index,
    /// and every comparison on it is refused as
    /// [`ConnectorError::Unsupported`](fabric_connector::ConnectorError) —
    /// fail-closed until `array_comparison` is implemented deliberately. It
    /// stays *selectable*: see
    /// [`collection_index`](crate::schema_index::SchemaIndex), which keeps the
    /// field and records only that it has no scalar type.
    pub(crate) fn named(&self) -> Option<&str> {
        match self {
            Self::Named { name } => Some(name),
            Self::Nullable { underlying_type } => underlying_type.named(),
            Self::Array { .. } | Self::Predicate { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_a_nullable_type() {
        let parsed: NdcType =
            serde_json::from_str(r#"{"type":"nullable","underlying_type":{"type":"named","name":"text"}}"#)
                .unwrap();

        assert_eq!(parsed.named(), Some("text"));
    }

    #[test]
    fn an_array_type_has_no_scalar_so_its_elements_operators_cannot_be_borrowed() {
        // The element's `_eq` is not the array's `_eq`. Handing back `int4`
        // here is what put a scalar operator on the wire against an array
        // column.
        let parsed: NdcType =
            serde_json::from_str(r#"{"type":"array","element_type":{"type":"named","name":"int4"}}"#)
                .unwrap();

        assert_eq!(parsed.named(), None);
    }

    #[test]
    fn a_nullable_array_is_still_an_array() {
        // The nullable wrapper comes off, and what is underneath is still not
        // a scalar — so the two unwrapping rules cannot be composed into one.
        let parsed: NdcType = serde_json::from_str(
            r#"{"type":"nullable","underlying_type":{"type":"array","element_type":{"type":"named","name":"text"}}}"#,
        )
        .unwrap();

        assert_eq!(parsed.named(), None);
    }

    #[test]
    fn a_predicate_type_has_no_scalar() {
        let parsed: NdcType =
            serde_json::from_str(r#"{"type":"predicate","object_type_name":"customers"}"#).unwrap();

        assert_eq!(parsed.named(), None);
    }
}
