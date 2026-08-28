//! What a connector declares about a procedure it exposes.
//!
//! # Why the arguments are modelled and not skipped
//!
//! They were skipped once. `procedures` was a list of bare names, so a schema
//! declaring `delete_customers(filter: predicate<customers>)` round-tripped to
//! `[{"name": "delete_customers"}]` and every configured argument name was
//! taken on trust. A mapping naming an argument the procedure never declares
//! passed startup validation, passed translation, and put the tenant predicate
//! on the wire under a name the connector had never heard of — which a
//! connector that ignores unknown arguments turns into an unscoped delete.
//!
//! `schema_response.jsonschema` requires `arguments` on every `ProcedureInfo`
//! and types each one, so nothing about that had to be inferred: the answer was
//! in a document already being parsed and thrown away.

use std::collections::BTreeMap;

use crate::wire::NdcType;

/// A procedure the connector exposes, and the arguments it declares.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcProcedureInfo {
    /// The procedure's name.
    pub(crate) name: String,

    /// The arguments it accepts, keyed by name.
    ///
    /// Required by the specification, but defaulted here rather than made
    /// mandatory: this crate reads a schema to find out what it may ask for,
    /// and refusing to parse an otherwise usable document over a missing key
    /// would turn one non-conforming procedure into a dead connector. An empty
    /// map is not a silent pass — it declares no arguments, so every configured
    /// argument name is checked against nothing and rejected.
    #[serde(default)]
    pub(crate) arguments: BTreeMap<String, NdcArgumentInfo>,
}

/// One declared argument.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcArgumentInfo {
    /// The argument's type.
    ///
    /// `predicate` is the one this crate can act on: it is what an argument
    /// carrying a filter must be, and an argument that is anything else cannot
    /// carry one. Everything a payload argument might be — an array of objects,
    /// an object, a connector-specific named type — is connector-defined, so
    /// the type is read but only the predicate distinction is enforced.
    #[serde(rename = "type")]
    pub(crate) argument_type: NdcType,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_arguments_a_procedure_declares() {
        let procedure: NdcProcedureInfo = serde_json::from_str(
            r#"{
                "name": "delete_customers",
                "arguments": {
                    "filter": {"type": {"type": "predicate", "object_type_name": "customers"}}
                },
                "result_type": {"type": "named", "name": "int4"}
            }"#,
        )
        .unwrap();

        assert_eq!(procedure.name, "delete_customers");
        assert!(matches!(
            procedure.arguments["filter"].argument_type,
            NdcType::Predicate { .. }
        ));
    }

    #[test]
    fn a_procedure_declaring_no_arguments_parses_as_declaring_none() {
        // Not the same as "declares whatever you configured". The check that
        // reads this treats an empty map as rejecting every configured name.
        let procedure: NdcProcedureInfo =
            serde_json::from_str(r#"{"name": "ping", "result_type": {"type": "named", "name": "int4"}}"#)
                .unwrap();

        assert!(procedure.arguments.is_empty());
    }
}
