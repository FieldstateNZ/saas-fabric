//! What `for_target` does to a write — scoping *and* stamping.

use serde_json::Value;

use crate::testing::{collection, discriminator_target, field, target_with};
use crate::{IsolationModel, MutationSpec, Row};

fn tenant_key() -> crate::FieldName {
    field("tenant_key")
}

#[test]
fn an_insert_is_stamped_with_the_tenant_discriminator() {
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new().with(field("name"), Value::String("Alice".into()))],
    };

    let MutationSpec::Insert { rows, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an insert");
    };

    assert_eq!(
        rows.first().unwrap().get(&tenant_key()),
        Some(&Value::String("tenant-482".to_owned()))
    );
}

#[test]
fn an_insert_cannot_claim_another_tenants_discriminator() {
    // The caller supplies a hostile value; it must be overwritten, not kept.
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new().with(tenant_key(), Value::String("tenant-999".into()))],
    };

    let MutationSpec::Insert { rows, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an insert");
    };

    assert_eq!(
        rows.first().unwrap().get(&tenant_key()),
        Some(&Value::String("tenant-482".to_owned()))
    );
}

#[test]
fn a_delete_with_no_predicate_is_scoped_to_the_tenant_not_the_table() {
    // Without this, "delete all" would empty the shared table for everyone.
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: None,
    };

    let MutationSpec::Delete { filter, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected a delete");
    };

    assert_eq!(filter, discriminator_target().isolation().tenant_predicate());
}

#[test]
fn an_update_is_both_scoped_and_stamped() {
    let spec = MutationSpec::Update {
        collection: collection(),
        filter: None,
        changes: Row::new().with(tenant_key(), Value::String("tenant-999".into())),
    };

    let MutationSpec::Update { filter, changes, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an update");
    };

    assert!(filter.is_some());
    // An update must not be able to move a row to another tenant.
    assert_eq!(
        changes.get(&tenant_key()),
        Some(&Value::String("tenant-482".to_owned()))
    );
}

#[test]
fn a_dedicated_database_mutation_is_unchanged() {
    let spec = MutationSpec::Delete {
        collection: collection(),
        filter: None,
    };

    assert_eq!(spec.for_target(&target_with(IsolationModel::Database)), spec);
}
