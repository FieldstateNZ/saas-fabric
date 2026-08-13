//! Validation happens where resources enter the registry, not at first use.
//!
//! `DataSource::validate` and `TenantRuntimeBinding::validate` both claim to
//! run "at load rather than at first request". Nothing on the load path called
//! either, so a tenant with no data bindings or a pool that could never hand
//! out a connection was installed in silence and failed — or quietly did not
//! fail — much later. These pin the claim down.
//!
//! Both the generic lifecycle and the two real resource types are covered: the
//! generic tests prove the registry does the right thing with a rejection, and
//! the concrete ones prove each type's `RegistryResource::validate` is wired to
//! the rule it is supposed to enforce, which no generic test can show.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, TenantId};

use crate::resource::registry::test_resource::{invalid_resource, registry, resource};
use crate::testing::{data_source, data_source_id, tenant, tenant_binding};
use crate::{DataSourceRegistry, PoolSettings, TenantRegistry, TenantRuntimeBinding};

#[test]
fn an_invalid_resource_never_enters_the_snapshot() {
    let registry = registry();

    let report = registry.apply_all(vec![invalid_resource("a", 1)]);

    assert_eq!(report.invalid_rejected, 1);
    assert_eq!(report.added, 0);
    assert!(registry.lookup(&"a".to_owned()).is_err());
}

#[test]
fn one_invalid_resource_does_not_stop_its_valid_neighbours_loading() {
    // The whole reason a rejection is a skip rather than a failed apply: one
    // operator's typo must not freeze every other resource's updates.
    let registry = registry();

    let report = registry.apply_all(vec![
        resource("good", 1),
        invalid_resource("bad", 1),
        resource("also-good", 1),
    ]);

    assert_eq!(report.added, 2);
    assert_eq!(report.invalid_rejected, 1);
    assert_eq!(registry.len(), 2);
    assert!(registry.lookup(&"good".to_owned()).is_ok());
    assert!(registry.lookup(&"also-good".to_owned()).is_ok());
}

#[test]
fn a_resource_that_turns_invalid_keeps_serving_the_copy_already_held() {
    // Absence from the incoming set is how deprovisioning is expressed; an
    // unusable payload is a reconciler bug. Treating the bug as a removal
    // would take a live tenant offline over a typo.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    let report = registry.apply_all(vec![invalid_resource("a", 2)]);

    assert_eq!(report.invalid_rejected, 1);
    assert_eq!(report.removed, 0);
    assert_eq!(report.updated, 0);
    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(1)
    );
}

#[test]
fn a_rejection_does_not_blank_the_registry() {
    // "A load failure must never become an empty set" has to hold for a set
    // that is entirely invalid, too.
    let registry = registry();
    registry.apply_all(vec![resource("a", 1), resource("b", 1)]);

    registry.apply_all(vec![invalid_resource("a", 2), invalid_resource("b", 2)]);

    assert!(registry.is_primed());
    assert_eq!(registry.len(), 2);
}

#[test]
fn a_rejection_is_not_movement_so_the_aggregate_log_stays_quiet() {
    // Nothing in the registry moved, and each rejection already gets its own
    // error-level log, so it must not also inflate the snapshot-applied line.
    let registry = registry();

    let report = registry.apply_all(vec![invalid_resource("a", 1)]);

    assert!(report.is_noop());
    assert_eq!(report.invalid_rejected, 1);
}

#[test]
fn applying_one_invalid_resource_is_refused() {
    let registry = registry();
    registry.apply_all(vec![resource("a", 1)]);

    assert!(!registry.apply_one(invalid_resource("a", 2)));
    assert_eq!(
        registry.lookup(&"a".to_owned()).unwrap().revision,
        BindingRevision::new(1)
    );
}

#[test]
fn an_incoherent_pool_does_not_take_a_data_source_out_of_the_registry() {
    // Deliberately inverted. Refusing a DataSource over `max_connections: 0`
    // resolved every tenant on it to `MissingDataSource` — a 500 — for a field
    // this process never reads and nothing in the workspace consumes. See
    // `DataSource::validate` for the full argument; the short version is that
    // the enforcement did more damage than the fault, and reconciliation is
    // what applies these numbers and so what should refuse them.
    let registry = DataSourceRegistry::new();
    let odd_pool = crate::DataSource {
        pool: PoolSettings {
            max_connections: 0,
            ..PoolSettings::default()
        },
        ..data_source("sql-au-east-03", 1)
    };

    let report = registry.apply_all(vec![odd_pool]);

    assert_eq!(report.invalid_rejected, 0);
    assert_eq!(report.added, 1);
    assert!(registry.lookup(&data_source_id("sql-au-east-03")).is_ok());
}

#[test]
fn the_pool_rule_itself_still_exists_for_whoever_applies_it() {
    // Demoting it from the load path must not delete it: reconciliation needs
    // the same check before it publishes a DataSource at all.
    assert!(PoolSettings {
        max_connections: 0,
        ..PoolSettings::default()
    }
    .validate()
    .is_err());
}

#[test]
fn a_tenant_with_no_data_bindings_is_rejected_at_load() {
    let registry = TenantRegistry::new();
    let unbound = TenantRuntimeBinding {
        data: BTreeMap::new(),
        ..tenant_binding("acme", 1, "sql-au-east-03")
    };

    let report = registry.apply_all(vec![unbound]);

    assert_eq!(report.invalid_rejected, 1);
    assert!(registry.lookup(&tenant("acme")).is_err());
}

#[test]
fn a_coherent_tenant_alongside_an_unbound_one_still_loads() {
    let registry = TenantRegistry::new();
    let unbound = TenantRuntimeBinding::new(TenantId::try_new("orphan").unwrap(), BindingRevision::new(1));

    let report = registry.apply_all(vec![tenant_binding("acme", 1, "sql-au-east-03"), unbound]);

    assert_eq!(report.added, 1);
    assert_eq!(report.invalid_rejected, 1);
    assert!(registry.lookup(&tenant("acme")).is_ok());
}
