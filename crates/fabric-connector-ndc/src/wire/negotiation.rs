//! `GET /capabilities` and `GET /schema` response types.
//!
//! Both are read once at startup. Nothing here is on the request path.

use std::collections::BTreeMap;

use serde_json::Value;

/// The body of `GET /capabilities`.
///
/// The nested capability objects are kept as raw JSON. NDC signals an optional
/// capability by the *presence* of a key rather than a boolean, and the set
/// grows between spec versions — modelling each one would mean a struct that
/// fails to deserialise every time Hasura adds a field. Presence checks are
/// both sufficient and forward-compatible.
///
/// Note what is **not** here: filtering, ordering, and paging. Those are core
/// NDC, required of every conforming connector, so there is nothing to
/// negotiate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcCapabilitiesResponse {
    /// The specification version the connector implements.
    pub(crate) version: String,

    /// The capability tree.
    #[serde(default)]
    pub(crate) capabilities: Value,
}

impl NdcCapabilitiesResponse {
    /// Whether the connector groups several mutations into one transaction.
    pub(crate) fn supports_transactional_mutations(&self) -> bool {
        self.capabilities
            .get("mutation")
            .and_then(|mutation| mutation.get("transactional"))
            .is_some()
    }
}

/// The body of `GET /schema`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcSchemaResponse {
    /// Scalar types, keyed by name. This is where operator semantics live.
    #[serde(default)]
    pub(crate) scalar_types: BTreeMap<String, NdcScalarType>,

    /// Object types, keyed by name.
    #[serde(default)]
    pub(crate) object_types: BTreeMap<String, NdcObjectType>,

    /// The collections the connector exposes.
    #[serde(default)]
    pub(crate) collections: Vec<NdcCollectionInfo>,

    /// Procedures available for mutations.
    #[serde(default)]
    pub(crate) procedures: Vec<NdcNamed>,
}

/// A scalar type and the operators defined over it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcScalarType {
    /// Operator name to its semantics.
    ///
    /// **This map is the portability mechanism.** The connector chooses the
    /// names — `_eq`, `eq`, `equals` — but declares what each one *means*, so
    /// the platform can find the right name for a semantic operator instead of
    /// hardcoding a vendor's spelling.
    #[serde(default)]
    pub(crate) comparison_operators: BTreeMap<String, NdcComparisonOperatorDefinition>,
}

/// What a comparison operator means.
///
/// The named variants are NDC's standard semantics. `Custom` covers everything
/// a connector invents; the platform cannot map a neutral operator onto a
/// custom one, so those are simply not offered.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcComparisonOperatorDefinition {
    /// Equality.
    Equal,
    /// Membership in a list.
    In,
    /// Strictly less than.
    LessThan,
    /// Less than or equal.
    LessThanOrEqual,
    /// Strictly greater than.
    GreaterThan,
    /// Greater than or equal.
    GreaterThanOrEqual,
    /// Substring containment.
    Contains,
    /// Case-insensitive containment.
    ContainsInsensitive,
    /// Prefix match.
    StartsWith,
    /// Case-insensitive prefix match.
    StartsWithInsensitive,
    /// Suffix match.
    EndsWith,
    /// Case-insensitive suffix match.
    EndsWithInsensitive,
    /// Anything connector-specific.
    #[serde(other)]
    Custom,
}

/// An object type's fields.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcObjectType {
    /// Field name to its definition.
    #[serde(default)]
    pub(crate) fields: BTreeMap<String, NdcObjectField>,
}

/// One field of an object type.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcObjectField {
    /// The field's type.
    #[serde(rename = "type")]
    pub(crate) field_type: NdcType,
}

/// A type reference.
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

/// A collection the connector exposes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcCollectionInfo {
    /// The collection's name.
    pub(crate) name: String,

    /// The name of the object type describing its rows.
    #[serde(rename = "type")]
    pub(crate) collection_type: String,
}

/// Anything in the schema identified only by name, such as a procedure.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcNamed {
    /// The name.
    pub(crate) name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_transactional_mutation_support_by_key_presence() {
        let response: NdcCapabilitiesResponse = serde_json::from_str(
            r#"{"version":"0.2.13","capabilities":{"query":{},"mutation":{"transactional":{}}}}"#,
        )
        .unwrap();

        assert!(response.supports_transactional_mutations());
    }

    #[test]
    fn a_connector_without_transactional_mutations_reports_false() {
        let response: NdcCapabilitiesResponse =
            serde_json::from_str(r#"{"version":"0.2.13","capabilities":{"query":{},"mutation":{}}}"#)
                .unwrap();

        assert!(!response.supports_transactional_mutations());
    }

    #[test]
    fn an_unrecognised_capability_key_does_not_break_deserialisation() {
        // Forward compatibility: a newer connector advertising something we
        // have never heard of must still be usable.
        let response: NdcCapabilitiesResponse = serde_json::from_str(
            r#"{"version":"0.2.13","capabilities":{"query":{"time_travel":{}},"invented":{}}}"#,
        )
        .unwrap();

        assert_eq!(response.version, "0.2.13");
    }

    #[test]
    fn unwraps_nullable_and_array_types_to_the_underlying_scalar() {
        let nullable: NdcType =
            serde_json::from_str(r#"{"type":"nullable","underlying_type":{"type":"named","name":"text"}}"#)
                .unwrap();
        let array: NdcType =
            serde_json::from_str(r#"{"type":"array","element_type":{"type":"named","name":"int4"}}"#)
                .unwrap();

        assert_eq!(nullable.named(), Some("text"));
        assert_eq!(array.named(), Some("int4"));
    }

    #[test]
    fn an_unknown_operator_semantic_falls_back_to_custom() {
        let definition: NdcComparisonOperatorDefinition =
            serde_json::from_str(r#"{"type":"matches_regex"}"#).unwrap();

        assert_eq!(definition, NdcComparisonOperatorDefinition::Custom);
    }

    #[test]
    fn parses_a_realistic_schema_document() {
        let schema: NdcSchemaResponse = serde_json::from_str(
            r#"{
                "scalar_types": {
                    "text": {
                        "aggregate_functions": {},
                        "comparison_operators": {
                            "_eq": {"type": "equal"},
                            "_like": {"type": "contains"}
                        }
                    }
                },
                "object_types": {
                    "customers": {
                        "fields": {
                            "id": {"type": {"type": "named", "name": "int4"}},
                            "name": {"type": {"type": "nullable", "underlying_type": {"type": "named", "name": "text"}}}
                        }
                    }
                },
                "collections": [
                    {"name": "customers", "type": "customers", "arguments": {}, "uniqueness_constraints": {}}
                ],
                "functions": [],
                "procedures": [{"name": "insert_customers"}]
            }"#,
        )
        .unwrap();

        assert_eq!(schema.collections.len(), 1);
        assert_eq!(
            schema.object_types["customers"].fields["name"].field_type.named(),
            Some("text")
        );
        assert_eq!(
            schema.scalar_types["text"].comparison_operators["_like"],
            NdcComparisonOperatorDefinition::Contains
        );
        assert_eq!(schema.procedures.first().unwrap().name, "insert_customers");
    }
}
