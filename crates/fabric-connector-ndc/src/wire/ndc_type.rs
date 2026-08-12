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
    /// The underlying named type, unwrapping nullability and arrays.
    ///
    /// A column declared `nullable(named "text")` still compares with `text`'s
    /// operators, so the wrappers have to come off before the operator lookup.
    pub(crate) fn named(&self) -> Option<&str> {
        match self {
            Self::Named { name } => Some(name),
            Self::Nullable { underlying_type } => underlying_type.named(),
            Self::Array { element_type } => element_type.named(),
            Self::Predicate { .. } => None,
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
    fn unwraps_an_array_type() {
        let parsed: NdcType =
            serde_json::from_str(r#"{"type":"array","element_type":{"type":"named","name":"int4"}}"#)
                .unwrap();

        assert_eq!(parsed.named(), Some("int4"));
    }

    #[test]
    fn a_predicate_type_has_no_scalar() {
        let parsed: NdcType =
            serde_json::from_str(r#"{"type":"predicate","object_type_name":"customers"}"#).unwrap();

        assert_eq!(parsed.named(), None);
    }
}
