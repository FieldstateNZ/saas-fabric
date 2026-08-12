//! Adversarial tests: a caller filter shaped to *widen* coverage — by `OR`,
//! by omission, or by naming another tenant's discriminator value directly —
//! still only ever narrows to this tenant once conjoined.

use crate::testing::{collection, discriminator_target, equals, tenant_predicate};
use crate::{Filter, MutationSpec, Row};

#[test]
fn an_update_with_an_or_filter_that_would_widen_beyond_the_tenant_still_conjoins_the_tenant_predicate() {
    // "my rows OR their rows" — after conjunction with the tenant predicate,
    // this can only ever match my rows, because a row cannot equal both
    // discriminator values at once.
    let widening = Filter::Or {
        clauses: vec![
            equals("tenant_key", "tenant-482"),
            equals("tenant_key", "tenant-999"),
        ],
    };

    let spec = MutationSpec::Update {
        collection: collection(),
        filter: Some(widening.clone()),
        changes: Row::new(),
    };

    let MutationSpec::Update { filter, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an update");
    };

    let Some(Filter::And { clauses }) = filter else {
        panic!("expected the caller filter conjoined with the tenant predicate");
    };
    assert!(clauses.contains(&widening));
    assert!(clauses.contains(&tenant_predicate()));
}

#[test]
fn a_delete_with_no_predicate_becomes_exactly_the_tenant_predicate_never_the_whole_table() {
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: None,
    };

    let MutationSpec::Delete { filter, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected a delete");
    };

    // Not just "some predicate" — exactly the tenant predicate and nothing
    // wider, since there was no caller clause to conjoin it with.
    assert_eq!(filter, Some(tenant_predicate()));
}

#[test]
fn a_delete_naming_another_tenants_discriminator_value_matches_nothing_not_their_rows() {
    let hostile = equals("tenant_key", "tenant-999");

    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: Some(hostile.clone()),
    };

    let MutationSpec::Delete { filter, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected a delete");
    };

    // Both equality clauses on the same column survive side by side. A real
    // backend evaluating `tenant_key = 'tenant-999' AND tenant_key =
    // 'tenant-482'` matches no row — which is the safe outcome, not a leak.
    let Some(Filter::And { clauses }) = filter else {
        panic!("expected both predicates present");
    };
    assert!(clauses.contains(&hostile));
    assert!(clauses.contains(&tenant_predicate()));
}
