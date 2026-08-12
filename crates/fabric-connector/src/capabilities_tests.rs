//! Capabilities refuse; they never degrade.

use serde_json::Value;

use crate::testing::{collection, field};
use crate::{
    ComparisonOperator, ConnectorCapabilities, ConnectorError, Filter, MutationSpec, QuerySpec, Row,
    SortField,
};

fn contains_filter() -> Filter {
    Filter::Compare {
        field: field("name"),
        operator: ComparisonOperator::Contains,
        value: Value::String("Ali".to_owned()),
    }
}

#[test]
fn the_baseline_accepts_a_simple_equality_query() {
    let spec = QuerySpec::new(collection()).with_filter(Filter::Compare {
        field: field("id"),
        operator: ComparisonOperator::Equal,
        value: Value::from(1),
    });

    assert!(ConnectorCapabilities::baseline()
        .ensure_supports_query(&spec)
        .is_ok());
}

#[test]
fn a_comparison_the_backend_cannot_express_is_refused_not_dropped() {
    let spec = QuerySpec::new(collection()).with_filter(contains_filter());

    let error = ConnectorCapabilities::baseline()
        .ensure_supports_query(&spec)
        .unwrap_err();

    assert!(matches!(error, ConnectorError::Unsupported { .. }));
}

#[test]
fn ordering_is_refused_when_the_backend_cannot_sort() {
    let capabilities = ConnectorCapabilities {
        ordering: false,
        ..ConnectorCapabilities::baseline()
    };
    let spec = QuerySpec::new(collection()).with_sort(vec![SortField::ascending(field("id"))]);

    assert!(capabilities.ensure_supports_query(&spec).is_err());
}

#[test]
fn a_read_only_connector_refuses_every_mutation() {
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new()],
    };

    assert!(ConnectorCapabilities::baseline()
        .ensure_supports_mutation(&spec)
        .is_err());
}

#[test]
fn a_delete_whose_predicate_the_backend_cannot_express_is_refused() {
    // Executing anyway would delete rows the predicate was meant to protect —
    // including, under discriminator isolation, other tenants'.
    let capabilities = ConnectorCapabilities {
        mutations: true,
        ..ConnectorCapabilities::baseline()
    };
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: Some(contains_filter()),
    };

    assert!(capabilities.ensure_supports_mutation(&spec).is_err());
}

#[test]
fn an_update_whose_predicate_the_backend_cannot_express_is_refused() {
    // Same hazard as the delete case: an update's filter is what stops it
    // reaching another tenant's rows under discriminator isolation. If the
    // backend cannot express it, approximating would be destructive, not
    // merely wrong.
    let capabilities = ConnectorCapabilities {
        mutations: true,
        ..ConnectorCapabilities::baseline()
    };
    let spec = MutationSpec::Update {
        collection: collection(),
        filter: Some(contains_filter()),
        changes: Row::new(),
    };

    assert!(capabilities.ensure_supports_mutation(&spec).is_err());
}

#[test]
fn an_update_or_delete_with_no_filter_is_not_refused_for_missing_capability() {
    // `for_target` guarantees a tenant predicate is always present by the
    // time a mutation reaches here under discriminator isolation, but this
    // check must not itself require a filter to exist — a legitimately
    // unfiltered mutation against a Database/Schema-isolated target is fine,
    // and capability checking should not invent a reason to reject it.
    let capabilities = ConnectorCapabilities {
        mutations: true,
        filtering: false,
        ..ConnectorCapabilities::baseline()
    };
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: None,
    };

    assert!(capabilities.ensure_supports_mutation(&spec).is_ok());
}
