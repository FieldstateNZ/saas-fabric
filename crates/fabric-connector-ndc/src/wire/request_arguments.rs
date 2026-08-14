//! The request-level arguments a connector declares in its schema.

use std::collections::BTreeMap;

use serde_json::Value;

/// What a connector says it will read off every request.
///
/// # Why this has to be read rather than assumed
///
/// Request-level arguments were added in NDC 0.2.4 and are how one request
/// tells a shared connector which tenant's connection to use — see the table
/// in [`crate`]'s docs. This declaration is the *only* thing distinguishing a
/// connector that routes per request from one that quietly serves everybody
/// the connection it was started with.
///
/// `schema/arguments.md` is silent on what a connector must do with a request
/// argument it never declared, and that silence is exactly why this matters:
/// the behaviour is connector-defined, and the one implementation that can be
/// read defines it as ignoring the argument. `ndc-postgres` v3.1.0 declares
/// nothing at all when statically configured, and its pool acquisition returns
/// the single static pool without consulting `request_arguments` — no error,
/// no warning, `200`.
///
/// `relational_query_arguments` is deliberately not modelled. The Data API
/// issues no relational queries, so an argument we would never send has
/// nothing to be checked against.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcRequestLevelArguments {
    /// Arguments every `POST /query` is expected to carry.
    ///
    /// The values are the specification's `ArgumentInfo` — a description and a
    /// type — and are kept opaque because only the declared *name* is
    /// load-bearing here. Modelling a body this crate never reads would add a
    /// way to fail parsing a schema that is otherwise perfectly usable, and
    /// buy nothing.
    #[serde(default)]
    pub(crate) query_arguments: BTreeMap<String, Value>,

    /// Arguments every `POST /mutation` is expected to carry.
    #[serde(default)]
    pub(crate) mutation_arguments: BTreeMap<String, Value>,
}

impl NdcRequestLevelArguments {
    /// Whether the connector reads this argument on `POST /query`.
    pub(crate) fn declares_for_query(&self, argument: &str) -> bool {
        self.query_arguments.contains_key(argument)
    }

    /// Whether the connector reads this argument on `POST /mutation`.
    ///
    /// Asked separately from [`Self::declares_for_query`] because the two are
    /// separate maps in the schema and a connector can genuinely declare one
    /// without the other — which would read one tenant's rows and write to
    /// another's.
    pub(crate) fn declares_for_mutation(&self, argument: &str) -> bool {
        self.mutation_arguments.contains_key(argument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(json: &str) -> NdcRequestLevelArguments {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn reads_the_argument_names_a_named_mode_connector_declares() {
        // The shape `ndc-postgres` returns for a name-routed deployment.
        let arguments = parsed(
            r#"{
                "query_arguments": {
                    "connection_name": {
                        "description": "The name of the connection to use.",
                        "type": {"type": "named", "name": "text"}
                    }
                },
                "mutation_arguments": {
                    "connection_name": {"type": {"type": "named", "name": "text"}}
                },
                "relational_query_arguments": {}
            }"#,
        );

        assert!(arguments.declares_for_query("connection_name"));
        assert!(arguments.declares_for_mutation("connection_name"));
        assert!(!arguments.declares_for_query("connection_string"));
    }

    #[test]
    fn an_argument_declared_only_for_queries_is_not_declared_for_mutations() {
        let arguments = parsed(
            r#"{
                "query_arguments": {"connection_name": {"type": {"type": "named", "name": "text"}}},
                "mutation_arguments": {},
                "relational_query_arguments": {}
            }"#,
        );

        assert!(arguments.declares_for_query("connection_name"));
        assert!(!arguments.declares_for_mutation("connection_name"));
    }
}
