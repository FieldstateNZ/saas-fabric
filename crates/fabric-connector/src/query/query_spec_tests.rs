//! What `for_target` does to a read.

use crate::testing::{collection, discriminator_target, equals, field, target_with, tenant_predicate};
use crate::{Filter, IsolationModel, QuerySpec, SchemaName};

fn caller_filter() -> Filter {
    equals("status", "active")
}

#[test]
fn a_dedicated_database_query_is_unchanged() {
    let spec = QuerySpec::new(collection()).with_filter(caller_filter());

    assert_eq!(spec.for_target(&target_with(IsolationModel::Database)), spec);
}

#[test]
fn a_schema_isolated_query_is_unchanged_because_the_connection_isolates_it() {
    let spec = QuerySpec::new(collection());

    let executed = spec.for_target(&target_with(IsolationModel::Schema {
        schema: SchemaName::try_new("acme").unwrap(),
    }));

    assert_eq!(executed.filter, None);
    assert_eq!(executed.collection, collection());
}

#[test]
fn a_discriminator_query_with_no_caller_filter_still_gets_the_tenant_predicate() {
    // Without this, an unfiltered list returns every tenant's rows.
    let executed = QuerySpec::new(collection()).for_target(&discriminator_target());

    assert_eq!(executed.filter, Some(tenant_predicate()));
}

#[test]
fn a_discriminator_query_conjoins_the_tenant_predicate_with_the_caller_filter() {
    let executed = QuerySpec::new(collection())
        .with_filter(caller_filter())
        .for_target(&discriminator_target());

    let Some(Filter::And { clauses }) = executed.filter else {
        panic!("expected the caller filter to be conjoined with the tenant predicate");
    };

    assert_eq!(clauses.len(), 2);
    assert!(clauses.contains(&caller_filter()));
}

#[test]
fn the_tenant_predicate_cannot_be_replaced_by_a_caller_filter_on_the_same_column() {
    // A caller filtering on the discriminator column must not widen its own
    // scope: both predicates survive, so the conjunction can only narrow.
    let hostile = equals("tenant_key", "tenant-999");

    let executed = QuerySpec::new(collection())
        .with_filter(hostile.clone())
        .for_target(&discriminator_target());

    let Some(Filter::And { clauses }) = executed.filter else {
        panic!("expected a conjunction");
    };

    assert!(clauses.contains(&hostile));
    assert!(clauses.contains(&tenant_predicate()));
}

#[test]
fn paging_and_projection_survive_targeting() {
    let spec = QuerySpec::new(collection())
        .with_fields(vec![field("id")])
        .with_paging(Some(10), Some(20));

    let executed = spec.for_target(&discriminator_target());

    assert_eq!(executed.limit, Some(10));
    assert_eq!(executed.offset, Some(20));
    assert_eq!(executed.fields.len(), 1);
}
