//! `GET /schema` response types.

use std::collections::BTreeMap;

use crate::wire::{NdcProcedureInfo, NdcRequestLevelArguments, NdcType};

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

    /// Procedures available for mutations, with the arguments each declares.
    ///
    /// The arguments are the load-bearing half. See
    /// [`NdcProcedureInfo`](crate::wire::NdcProcedureInfo) for what modelling
    /// only the name cost.
    #[serde(default)]
    pub(crate) procedures: Vec<NdcProcedureInfo>,

    /// The request-level arguments the connector requires.
    ///
    /// Declared `anyOf [RequestLevelArguments, null]`, so `None` covers both an
    /// absent key and an explicit `null` — and both say the same thing: this
    /// connector declares none, so nothing sent in `request_arguments` is
    /// promised to have any effect on it.
    ///
    /// Checked against the configuration's routing at startup, in
    /// `registration::routing_arguments`. Reading it and doing nothing with it
    /// would be no better than not reading it.
    #[serde(default)]
    pub(crate) request_arguments: Option<NdcRequestLevelArguments>,
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
/// The named variants are NDC's standard semantics. `Custom` catches everything
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

/// A collection the connector exposes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcCollectionInfo {
    /// The collection's name.
    pub(crate) name: String,

    /// The name of the object type describing its rows.
    #[serde(rename = "type")]
    pub(crate) collection_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads a real `ndc-postgres` v3.1.0 document, checked in under
    /// `tests/fixtures/` -- see the README there for how it was captured.
    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/ndc-postgres-v3.1.0/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).unwrap()
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
                        "comparison_operators": {"_eq": {"type": "equal"}, "_like": {"type": "contains"}}
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
                "procedures": [{
                    "name": "insert_customers",
                    "arguments": {"objects": {"type": {"type": "array", "element_type": {"type": "named", "name": "customers"}}}},
                    "result_type": {"type": "named", "name": "int4"}
                }]
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

    #[test]
    fn a_custom_operator_carrying_an_argument_type_parses() {
        // R1: `Custom` is `#[serde(other)]` on an internally-tagged enum, and
        // the only test of it before this one used a bare `{"type": "..."}`
        // with no sibling fields. The real connector's `_ilike` operator
        // carries an `argument_type` alongside `type`, which is exactly the
        // shape a unit variant has to tolerate rather than choke on.
        let schema: NdcSchemaResponse = serde_json::from_str(&fixture("schema-static.json")).unwrap();

        assert_eq!(
            schema.scalar_types["text"].comparison_operators["_ilike"],
            NdcComparisonOperatorDefinition::Custom
        );
    }

    #[test]
    fn a_schema_with_a_top_level_capabilities_member_still_parses() {
        // The real `/schema` response carries a top-level `capabilities`
        // member (`{"query":{"aggregates":{"count_scalar_type":"int8"}}}`)
        // that `NdcSchemaResponse` does not model at all. Harmless only
        // because nothing here sets `deny_unknown_fields` -- this pins that
        // the document still parses rather than being asserted about.
        let schema: NdcSchemaResponse = serde_json::from_str(&fixture("schema-static.json")).unwrap();

        assert_eq!(
            schema
                .collections
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["articles"]
        );
    }
}
