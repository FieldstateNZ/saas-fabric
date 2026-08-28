//! `POST /mutation` request and response types.

use std::collections::BTreeMap;

use serde_json::Value;

/// The body of `POST /mutation`.
///
/// # Mutations are procedure calls
///
/// This is the part of NDC that surprises people coming from a CRUD API. Core
/// NDC 0.2 has no generic insert/update/delete: the only mutation operation is
/// **invoking a procedure** that the connector declares in its schema.
/// `ndc-postgres`, for instance, generates procedures like `insert_customers`
/// from its configuration.
///
/// That is a deliberate specification choice — it lets a connector expose
/// exactly the writes its backend can do safely, rather than pretending every
/// datastore supports the same write model. The consequence for us is that
/// mapping a neutral [`MutationSpec`](fabric_connector::MutationSpec) onto a
/// procedure call requires per-collection configuration
/// ([`CollectionProcedures`](crate::CollectionProcedures)), because procedure
/// names and argument shapes are the connector's to choose.
///
/// Where no mapping is configured, the connector reports `mutations: false` and
/// writes are refused. Guessing a procedure name and hoping is not an option
/// when the operation might be a delete.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcMutationRequest {
    /// The operations to perform, in order.
    pub(crate) operations: Vec<NdcMutationOperation>,

    /// Relationships involved. Always empty for us.
    pub(crate) collection_relationships: BTreeMap<String, Value>,

    /// Request-level arguments — the per-tenant connection routing, exactly as
    /// on a query.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_arguments: Option<BTreeMap<String, Value>>,
}

/// A single mutation operation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcMutationOperation {
    /// Invoke a procedure.
    Procedure {
        /// The procedure's name in the connector's schema.
        name: String,
        /// Named arguments.
        arguments: BTreeMap<String, Value>,
        /// Fields to return, or `None` for everything.
        #[serde(skip_serializing_if = "Option::is_none")]
        fields: Option<Value>,
    },
}

/// The body of a successful `POST /mutation` response.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NdcMutationResponse {
    /// One result per operation, in request order.
    pub(crate) operation_results: Vec<NdcOperationResult>,
}

/// The result of one operation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum NdcOperationResult {
    /// A procedure's return value. Its shape is the procedure's to decide.
    Procedure {
        /// Whatever the procedure returned.
        result: Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mutation_request_serialises_as_a_procedure_invocation() {
        let request = NdcMutationRequest {
            operations: vec![NdcMutationOperation::Procedure {
                name: "insert_customers".to_owned(),
                arguments: BTreeMap::from([("objects".to_owned(), Value::Array(vec![]))]),
                fields: None,
            }],
            collection_relationships: BTreeMap::new(),
            request_arguments: None,
        };

        let json = serde_json::to_value(&request).unwrap();

        assert_eq!(json["operations"][0]["type"], "procedure");
        assert_eq!(json["operations"][0]["name"], "insert_customers");
        assert!(json["operations"][0]["arguments"]["objects"].is_array());
    }

    #[test]
    fn deserialises_a_procedure_result() {
        let response: NdcMutationResponse = serde_json::from_str(
            r#"{"operation_results":[{"type":"procedure","result":{"affected_rows":3}}]}"#,
        )
        .unwrap();

        let NdcOperationResult::Procedure { result } = response.operation_results.first().unwrap();
        assert_eq!(result["affected_rows"], 3);
    }
}
