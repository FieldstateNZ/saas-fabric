//! What the connector told us it can do, in neutral terms.

use fabric_connector::ConnectorCapabilities;

use crate::wire::NdcCapabilitiesResponse;
use crate::{NdcConnectorConfig, SchemaIndex};

/// Builds the neutral capability set for a connector.
///
/// Three sources combine here:
///
/// - **The protocol.** Filtering, ordering, and paging are core NDC, required
///   of every conforming connector, so they are unconditionally true. They are
///   not negotiated and there is no capability key for them.
/// - **The capabilities response.** Optional extras such as transactional
///   mutations, signalled by key presence.
/// - **Our own configuration.** Writes require a procedure mapping
///   ([`CollectionProcedures`](crate::CollectionProcedures)), so a connector
///   that could accept writes still reports `mutations: false` until one is
///   configured. That is the fail-closed direction.
pub(crate) fn to_capabilities(
    capabilities: &NdcCapabilitiesResponse,
    index: &SchemaIndex,
    config: &NdcConnectorConfig,
) -> ConnectorCapabilities {
    ConnectorCapabilities {
        filtering: true,
        ordering: true,
        paging: true,
        mutations: config.has_writes(),
        transactional_mutations: capabilities.supports_transactional_mutations(),
        // The Data API does not issue aggregate queries, so even where the
        // connector could count, the platform does not ask it to.
        total_count: false,
        comparisons: index.supported_operators().clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use fabric_connector::ComparisonOperator;

    use super::*;
    use crate::config::{CollectionProcedures, ProcedureBinding};
    use crate::wire::NdcSchemaResponse;

    fn index() -> SchemaIndex {
        let schema: NdcSchemaResponse = serde_json::from_str(
            r#"{
                "scalar_types": {"text": {"comparison_operators": {"_eq": {"type": "equal"}}}},
                "object_types": {"customers": {"fields": {"id": {"type": {"type": "named", "name": "text"}}}}},
                "collections": [{"name": "customers", "type": "customers"}],
                "procedures": [{"name": "insert_customers"}]
            }"#,
        )
        .unwrap();

        SchemaIndex::build(&schema)
    }

    fn capabilities(json: &str) -> NdcCapabilitiesResponse {
        serde_json::from_str(json).unwrap()
    }

    fn config(procedures: BTreeMap<String, CollectionProcedures>) -> NdcConnectorConfig {
        NdcConnectorConfig::for_test(procedures)
    }

    #[test]
    fn core_protocol_features_are_always_available() {
        let result = to_capabilities(
            &capabilities(r#"{"version":"0.2.13","capabilities":{}}"#),
            &index(),
            &config(BTreeMap::new()),
        );

        assert!(result.filtering);
        assert!(result.ordering);
        assert!(result.paging);
    }

    #[test]
    fn writes_stay_disabled_until_a_procedure_mapping_exists() {
        // The connector exposes `insert_customers`, but without a mapping the
        // platform still refuses writes.
        let result = to_capabilities(
            &capabilities(r#"{"version":"0.2.13","capabilities":{"mutation":{}}}"#),
            &index(),
            &config(BTreeMap::new()),
        );

        assert!(!result.mutations);
    }

    #[test]
    fn a_configured_mapping_enables_writes() {
        let procedures = BTreeMap::from([(
            "customers".to_owned(),
            CollectionProcedures {
                insert: Some(ProcedureBinding {
                    procedure: "insert_customers".to_owned(),
                    payload_argument: Some("objects".to_owned()),
                    filter_argument: None,
                }),
                ..CollectionProcedures::default()
            },
        )]);

        let result = to_capabilities(
            &capabilities(r#"{"version":"0.2.13","capabilities":{"mutation":{}}}"#),
            &index(),
            &config(procedures),
        );

        assert!(result.mutations);
    }

    #[test]
    fn transactional_mutations_follow_the_capabilities_response() {
        let result = to_capabilities(
            &capabilities(r#"{"version":"0.2.13","capabilities":{"mutation":{"transactional":{}}}}"#),
            &index(),
            &config(BTreeMap::new()),
        );

        assert!(result.transactional_mutations);
    }

    #[test]
    fn comparisons_come_from_the_schema_not_from_a_hardcoded_list() {
        let result = to_capabilities(
            &capabilities(r#"{"version":"0.2.13","capabilities":{}}"#),
            &index(),
            &config(BTreeMap::new()),
        );

        assert!(result.comparisons.contains(&ComparisonOperator::Equal));
        assert!(!result.comparisons.contains(&ComparisonOperator::GreaterThan));
    }
}
