//! Capabilities refuse; they never degrade.

use std::collections::BTreeSet;

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

fn membership_filter() -> Filter {
    Filter::In {
        field: field("region"),
        values: vec![Value::String("au".to_owned()), Value::String("nz".to_owned())],
    }
}

fn null_filter() -> Filter {
    Filter::IsNull {
        field: field("archived_at"),
    }
}

/// A backend that can filter but declares no comparison at all — the case a
/// membership predicate must not slip past.
fn no_comparisons() -> ConnectorCapabilities {
    ConnectorCapabilities {
        comparisons: BTreeSet::new(),
        ..ConnectorCapabilities::baseline()
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

#[test]
fn a_membership_predicate_is_refused_when_equality_is_undeclared() {
    // `In` is served as a disjunction of equalities, so a backend with no
    // equality operator cannot express it. Reporting no requirement — the
    // former behaviour — meant only the coarse `filtering` flag was consulted
    // and the query was executed with the predicate silently widened.
    let spec = QuerySpec::new(collection()).with_filter(membership_filter());

    let error = no_comparisons().ensure_supports_query(&spec).unwrap_err();

    assert!(matches!(error, ConnectorError::Unsupported { .. }));
}

#[test]
fn a_membership_predicate_is_accepted_when_equality_is_declared() {
    // The refusal must be a real check, not a blanket ban on `In`.
    let spec = QuerySpec::new(collection()).with_filter(membership_filter());

    assert!(ConnectorCapabilities::baseline()
        .ensure_supports_query(&spec)
        .is_ok());
}

#[test]
fn a_delete_whose_membership_predicate_is_inexpressible_is_refused() {
    // The destructive case: widening `region IN (au, nz)` to "every row"
    // deletes rows the caller never asked to touch, and under discriminator
    // isolation the same widening reaches other tenants.
    let capabilities = ConnectorCapabilities {
        mutations: true,
        ..no_comparisons()
    };
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: Some(membership_filter()),
    };

    assert!(capabilities.ensure_supports_mutation(&spec).is_err());
}

#[test]
fn a_null_test_is_refused_by_a_backend_that_has_not_declared_null_checks() {
    let spec = QuerySpec::new(collection()).with_filter(null_filter());

    let error = ConnectorCapabilities::baseline()
        .ensure_supports_query(&spec)
        .unwrap_err();

    assert!(matches!(error, ConnectorError::Unsupported { .. }));
}

#[test]
fn a_null_test_is_accepted_once_the_backend_declares_null_checks() {
    let capabilities = ConnectorCapabilities {
        null_checks: true,
        ..ConnectorCapabilities::baseline()
    };
    let spec = QuerySpec::new(collection()).with_filter(null_filter());

    assert!(capabilities.ensure_supports_query(&spec).is_ok());
}

#[test]
fn an_update_whose_null_test_is_inexpressible_is_refused() {
    let capabilities = ConnectorCapabilities {
        mutations: true,
        ..ConnectorCapabilities::baseline()
    };
    let spec = MutationSpec::Update {
        collection: collection(),
        filter: Some(null_filter()),
        changes: Row::new(),
    };

    assert!(capabilities.ensure_supports_mutation(&spec).is_err());
}

#[test]
fn an_inexpressible_clause_nested_under_a_negation_is_still_refused() {
    // The walk must reach every clause. A check that only inspected the top
    // level would pass this and then run a predicate whose `IsNull` arm the
    // backend cannot evaluate.
    let spec = QuerySpec::new(collection()).with_filter(Filter::Not {
        clause: Box::new(Filter::Or {
            clauses: vec![null_filter()],
        }),
    });

    assert!(ConnectorCapabilities::baseline()
        .ensure_supports_query(&spec)
        .is_err());
}

#[test]
fn declaring_null_checks_does_not_excuse_an_undeclared_comparison() {
    // The two checks are independent; neither may satisfy the other.
    let capabilities = ConnectorCapabilities {
        null_checks: true,
        ..no_comparisons()
    };
    let spec = QuerySpec::new(collection()).with_filter(membership_filter());

    assert!(capabilities.ensure_supports_query(&spec).is_err());
}
