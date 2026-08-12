//! Adversarial tests: negating the tenant column is the most direct attempt
//! to see another tenant — `NOT (tenant_key = mine)` reads as "everything
//! that isn't mine". The tenant predicate must still end up conjoined, so
//! the result is the empty set, never someone else's rows. Also checks the
//! same hostile filter is inert where isolation is structural, since that is
//! the case §18 relies on for everything except discriminator isolation.

use crate::testing::{collection, discriminator_target, equals, target_with, tenant_predicate};
use crate::{Filter, IsolationModel, MutationSpec, Row, SchemaName};

fn not_mine() -> Filter {
    Filter::Not {
        clause: Box::new(equals("tenant_key", "tenant-482")),
    }
}

#[test]
fn a_not_mine_filter_on_an_update_still_conjoins_the_tenant_predicate() {
    let spec = MutationSpec::Update {
        collection: collection(),
        filter: Some(not_mine()),
        changes: Row::new(),
    };

    let MutationSpec::Update { filter, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an update");
    };

    let Some(Filter::And { clauses }) = filter else {
        panic!("expected a conjunction");
    };
    assert!(clauses.contains(&not_mine()));
    assert!(clauses.contains(&tenant_predicate()));
}

#[test]
fn a_not_mine_filter_on_a_delete_still_conjoins_the_tenant_predicate() {
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: Some(not_mine()),
    };

    let MutationSpec::Delete { filter, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected a delete");
    };

    let Some(Filter::And { clauses }) = filter else {
        panic!("expected a conjunction");
    };
    assert!(clauses.contains(&not_mine()));
    assert!(clauses.contains(&tenant_predicate()));
}

#[test]
fn a_not_mine_filter_is_inert_under_database_isolation() {
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: Some(not_mine()),
    };

    // Isolation is structural for a dedicated database, so `for_target` has
    // nothing to add — the caller's filter passes through unchanged, exactly
    // as it would for a well-behaved caller.
    assert_eq!(spec.for_target(&target_with(IsolationModel::Database)), spec);
}

#[test]
fn a_not_mine_filter_is_inert_under_schema_isolation() {
    let spec = MutationSpec::Update {
        collection: collection(),
        filter: Some(not_mine()),
        changes: Row::new(),
    };

    let schema = SchemaName::try_new("acme").expect("valid schema name");
    assert_eq!(
        spec.for_target(&target_with(IsolationModel::Schema { schema })),
        spec
    );
}
