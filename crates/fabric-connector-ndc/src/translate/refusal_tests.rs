//! What a refusal is allowed to tell an application, and what it owes an
//! operator.
//!
//! `fabric-data-api` forwards `ConnectorError::Unsupported.feature` in a 400
//! body — it is the only connector error text that is not masked. Since that
//! field became a closed `UnsupportedFeature`, a leak through it is a compile
//! error rather than something to test for. What still needs driving through
//! the *real* translator is the pair of properties the type cannot hold on its
//! own: that the capability a caller is told is the actionable one, and that
//! the identifiers it deliberately omits were not simply thrown away.

use std::collections::BTreeMap;

use fabric_connector::{
    CollectionName, ConnectionSelector, ConnectorCapabilities, ConnectorError, ConnectorId, ExecutionTarget,
    FieldName, IsolationModel, MutationSpec, QuerySpec, Row,
};
use fabric_core::{BindingRevision, DataSourceId, TenantId};

use crate::config::{CollectionProcedures, ProcedureBinding};
use crate::translate::{to_capabilities, to_mutation_request, to_query_request};
use crate::wire::{NdcCapabilitiesResponse, NdcSchemaResponse};
use crate::{NdcConnectorConfig, SchemaIndex};

/// The physical names that must never appear in anything a caller is shown.
const TABLE: &str = "customer_records_v2";
const DISCRIMINATOR: &str = "tenant_key";

/// A connector schema arranged as the leak needs it.
///
/// The shared table is keyed on `tenant_key`, whose scalar type declares only
/// case-insensitive containment — no equality. `name` is plain `text` and does
/// declare equality, so `SchemaIndex::supported_operators` (a *union* across
/// scalar types) still reports `Equal` and the capability gate lets the
/// operation through to translation, where the per-field lookup then fails.
fn schema() -> NdcSchemaResponse {
    serde_json::from_str(
        r#"{
            "scalar_types": {
                "text": {"comparison_operators": {"_eq": {"type": "equal"}}},
                "citext": {"comparison_operators": {"_ilike": {"type": "contains_insensitive"}}}
            },
            "object_types": {"customer_records_v2": {"fields": {
                "name": {"type": {"type": "named", "name": "text"}},
                "tenant_key": {"type": {"type": "named", "name": "citext"}}
            }}},
            "collections": [{"name": "customer_records_v2", "type": "customer_records_v2"}],
            "procedures": [{"name": "update_customer_records_v2"}]
        }"#,
    )
    .unwrap()
}

fn index() -> SchemaIndex {
    SchemaIndex::build(&schema())
}

/// A tenant on discriminator isolation — the model where the platform's own
/// predicate, not the connection, is the tenant boundary.
fn target() -> ExecutionTarget {
    ExecutionTarget::new(
        TenantId::try_new("acme").unwrap(),
        BindingRevision::new(42),
        DataSourceId::try_new("sql-au-east-03").unwrap(),
        BindingRevision::new(7),
        ConnectorId::try_new("pg").unwrap(),
        ConnectionSelector::Default,
        IsolationModel::Discriminator {
            column: FieldName::try_new(DISCRIMINATOR).unwrap(),
            value: "tenant-482".to_owned(),
        },
    )
}

fn collection() -> CollectionName {
    CollectionName::try_new(TABLE).unwrap()
}

fn capabilities(index: &SchemaIndex, config: &NdcConnectorConfig) -> ConnectorCapabilities {
    let response: NdcCapabilitiesResponse =
        serde_json::from_str(r#"{"version": "0.2.13", "capabilities": {"query": {}, "mutation": {}}}"#)
            .unwrap();

    to_capabilities(&response, index, config)
}

/// The published half of the error — what `public_message` would forward.
fn published(error: &ConnectorError) -> &'static str {
    let ConnectorError::Unsupported { feature, .. } = error else {
        panic!("expected Unsupported, got {error:?}");
    };
    feature.as_str()
}

/// The operator's half — what only a log line sees.
fn recorded(error: &ConnectorError) -> String {
    error.operator_message()
}

fn assert_names_nothing_physical(feature: &str) {
    assert!(!feature.contains(TABLE), "leaked the table name: {feature}");
    assert!(
        !feature.contains(DISCRIMINATOR),
        "leaked the isolation column: {feature}"
    );
}

#[test]
fn a_predicate_refusal_names_the_comparison_not_the_isolation_column() {
    // The worst case. `for_target` conjoins the discriminator, so translation
    // refuses on a predicate the caller never wrote, over the one column that
    // is holding their tenant boundary up.
    let index = index();
    let config = NdcConnectorConfig::for_test(BTreeMap::new());
    let spec = QuerySpec::new(collection()).for_target(&target());

    // Reachability: the capability gate the connector runs first passes,
    // because `comparisons` is the permissive union across scalar types.
    capabilities(&index, &config)
        .ensure_supports_query(&spec)
        .expect("the union of operators contains Equal, so the gate lets this through");

    let error = to_query_request(&spec, None, &index).unwrap_err();

    assert_names_nothing_physical(published(&error));
    assert_eq!(published(&error), "the equal comparison");

    // The other half of the bargain: an operator can still see which column
    // stopped the translation, because it went to the refusal detail.
    let recorded = recorded(&error);
    assert!(recorded.contains(TABLE), "{recorded}");
    assert!(recorded.contains(DISCRIMINATOR), "{recorded}");
}

#[test]
fn a_refused_comparison_still_says_which_comparison() {
    // The counterweight to the test above: masking must not go so far that an
    // authorised caller cannot tell what to change.
    let filter = fabric_connector::Filter::Compare {
        field: FieldName::try_new("name").unwrap(),
        operator: fabric_connector::ComparisonOperator::Contains,
        value: serde_json::Value::String("ali".to_owned()),
    };
    let spec = QuerySpec::new(collection()).with_filter(filter);

    let feature = published(&to_query_request(&spec, None, &index()).unwrap_err());

    assert_eq!(feature, "the contains comparison");
}

#[test]
fn an_unmapped_collection_refusal_names_the_capability_not_the_table() {
    let config = NdcConnectorConfig::for_test(BTreeMap::new());
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new()],
    }
    .for_target(&target());

    let error = to_mutation_request(&spec, None, &config, &index()).unwrap_err();

    assert_names_nothing_physical(published(&error));
    assert_eq!(published(&error), "writes to this collection");
    assert!(recorded(&error).contains(TABLE));
}

#[test]
fn an_unmapped_verb_refusal_names_the_capability_not_the_table() {
    let config = NdcConnectorConfig::for_test(BTreeMap::from([(
        TABLE.to_owned(),
        CollectionProcedures {
            update: Some(ProcedureBinding {
                procedure: "update_customer_records_v2".to_owned(),
                payload_argument: Some("set".to_owned()),
                filter_argument: Some("filter".to_owned()),
            }),
            ..CollectionProcedures::default()
        },
    )]));

    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: None,
    }
    .for_target(&target());

    let error = to_mutation_request(&spec, None, &config, &index()).unwrap_err();

    assert_names_nothing_physical(published(&error));
    assert_eq!(published(&error), "deletes on this collection");
    assert!(recorded(&error).contains(TABLE));
}

/// Every `Unsupported` this crate can raise, checked in one place.
///
/// Now a belt to the type's braces rather than the guard itself: a new
/// construction site cannot name a table, because `UnsupportedFeature` has
/// nowhere to put one. What this still earns is the reverse direction — that
/// each site chose a capability name a caller can act on.
#[test]
fn no_refusal_this_crate_raises_names_anything_physical() {
    let index = index();
    let empty = NdcConnectorConfig::for_test(BTreeMap::new());

    let query = QuerySpec::new(collection()).for_target(&target());
    let insert = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new()],
    };

    let errors = [
        to_query_request(&query, None, &index).unwrap_err(),
        to_mutation_request(&insert, None, &empty, &index).unwrap_err(),
    ];

    for error in &errors {
        assert_names_nothing_physical(published(error));
    }
}
