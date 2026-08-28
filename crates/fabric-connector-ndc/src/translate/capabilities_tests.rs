//! Tests for capabilities.

use super::capabilities::*;
use crate::config::{CollectionProcedures, ProcedureBinding};
use crate::wire::NdcCapabilitiesResponse;
use crate::wire::NdcSchemaResponse;
use crate::{NdcConnectorConfig, SchemaIndex};
use fabric_connector::ComparisonOperator;
use std::collections::BTreeMap;

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
