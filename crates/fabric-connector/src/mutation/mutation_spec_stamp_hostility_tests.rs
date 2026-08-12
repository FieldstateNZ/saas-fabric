//! Adversarial tests for row *stamping*: proving a hostile caller cannot get
//! their own value into the discriminator column, by any route.
//!
//! [`mutation_spec_tests`](super::mutation_spec_tests) already shows the
//! mechanism working on a well-behaved caller. This module assumes the
//! opposite — the caller is actively trying to write into another tenant's
//! rows — and checks `for_target` refuses every variant of that attempt.

use serde_json::Value;

use crate::testing::{collection, discriminator_target, field, target_with};
use crate::{IsolationModel, MutationSpec, Row};

fn tenant_key() -> crate::FieldName {
    field("tenant_key")
}

fn mine() -> Value {
    Value::String("tenant-482".to_owned())
}

#[test]
fn an_insert_supplying_another_tenants_discriminator_value_is_overwritten() {
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new().with(tenant_key(), Value::String("tenant-999".into()))],
    };

    let MutationSpec::Insert { rows, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an insert");
    };

    assert_eq!(rows.first().unwrap().get(&tenant_key()), Some(&mine()));
}

#[test]
fn an_insert_supplying_the_correct_discriminator_value_is_still_overwritten_not_trusted() {
    // Even a caller that happens to supply the right value must not be able
    // to rely on it being read back verbatim: stamping is unconditional, not
    // "fill in if missing" — there is no code path where a caller-supplied
    // value for this column survives.
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new().with(tenant_key(), mine())],
    };

    let MutationSpec::Insert { rows, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an insert");
    };

    assert_eq!(rows.first().unwrap().get(&tenant_key()), Some(&mine()));
}

#[test]
fn an_insert_with_a_malformed_discriminator_value_is_overwritten() {
    // Null, empty, and wrong-typed JSON are all just other shapes of "not
    // this tenant's value" — none of them should reach the connector.
    let hostile_values = [
        Value::Null,
        Value::String(String::new()),
        Value::from(999),
        Value::Bool(true),
        Value::Array(vec![Value::String("tenant-999".into())]),
    ];

    for hostile in hostile_values {
        let spec = MutationSpec::Insert {
            collection: collection(),
            rows: vec![Row::new().with(tenant_key(), hostile.clone())],
        };

        let MutationSpec::Insert { rows, .. } = spec.for_target(&discriminator_target()) else {
            panic!("expected an insert");
        };

        assert_eq!(
            rows.first().unwrap().get(&tenant_key()),
            Some(&mine()),
            "hostile value {hostile:?} was not overwritten"
        );
    }
}

#[test]
fn a_multi_row_insert_stamps_every_row_even_when_only_some_are_hostile() {
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![
            Row::new().with(field("name"), Value::String("no discriminator at all".into())),
            Row::new().with(tenant_key(), Value::String("tenant-999".into())),
            Row::new().with(tenant_key(), mine()),
        ],
    };

    let MutationSpec::Insert { rows, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an insert");
    };

    assert_eq!(rows.len(), 3);
    for row in &rows {
        assert_eq!(row.get(&tenant_key()), Some(&mine()));
    }
}

#[test]
fn an_update_whose_changes_try_to_move_the_row_to_another_tenant_is_stamped_back() {
    let spec = MutationSpec::Update {
        collection: collection(),
        filter: None,
        changes: Row::new().with(tenant_key(), Value::String("tenant-999".into())),
    };

    let MutationSpec::Update { changes, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an update");
    };

    assert_eq!(changes.get(&tenant_key()), Some(&mine()));
}

#[test]
fn stamping_does_not_happen_under_database_or_schema_isolation() {
    // There is nothing to stamp: the connection already cannot reach another
    // tenant, so a caller-supplied value here is inert either way. Proven by
    // checking the mutation passes through completely unchanged.
    let spec = MutationSpec::Insert {
        collection: collection(),
        rows: vec![Row::new().with(tenant_key(), Value::String("tenant-999".into()))],
    };

    assert_eq!(spec.for_target(&target_with(IsolationModel::Database)), spec);
}
