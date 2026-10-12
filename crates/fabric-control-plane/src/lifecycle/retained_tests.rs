//! The retained-placement check a restore passes, rule by rule, against the
//! publication the target holds.

use fabric_client_model::{Client, ClientDocument};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{IsolationModelDocument, PlacementRecord};
use fabric_runtime_publication::{PublishedResources, ResourceHistory, TenantBindingDocument};
use serde_json::json;

use super::retained::{check, RetainedEvidence};
use crate::RestoreRefusal;

fn client(data: &str) -> Client {
    ClientDocument::parse(&format!(
        "apiVersion: fabric.fieldstate.nz/v2
kind: Client
metadata:
  name: acme
spec:
  displayName: Acme
  identity:
    realm: acme
    roles: [Client Realm Administrator, Client Realm User]
    clients: []
{data}"
    ))
    .unwrap()
    .into_client()
}

fn two_intents() -> Client {
    client("  data:\n    primary:\n      class: dedicated\n    audit:\n      class: dedicated\n")
}

fn record(tenant: &str, logical: &str, revision: u64, data_source: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).unwrap(),
        logical: LogicalDataSourceName::try_new(logical).unwrap(),
        revision: BindingRevision::new(revision),
        data_source: DataSourceId::try_new(data_source).unwrap(),
        isolation: IsolationModelDocument::Database {},
        placed_at: "2026-10-11T00:00:00Z".to_owned(),
    }
}

fn placed() -> Vec<PlacementRecord> {
    vec![
        record("acme", "primary", 2, "shared-a"),
        record("acme", "audit", 1, "shared-a"),
        record("globex", "primary", 9, "shared-a"),
    ]
}

/// `acme` held at `revision`, bound as `placed` binds it.
fn held(revision: u64) -> TenantBindingDocument {
    let binding = json!({"data_source": "shared-a", "isolation": IsolationModelDocument::Database {}});
    serde_json::from_value(json!({
        "tenant": "acme",
        "revision": revision,
        "data": {"primary": binding, "audit": binding},
    }))
    .unwrap()
}

fn published(tenants: Vec<TenantBindingDocument>, frontier: Option<u64>) -> PublishedResources {
    let mut history = ResourceHistory::default();
    if let Some(revision) = frontier {
        history
            .tenants
            .insert(TenantId::try_new("acme").unwrap(), BindingRevision::new(revision));
    }
    PublishedResources {
        tenants,
        data_sources: Vec::new(),
        catalog: None,
        history,
    }
}

fn read(placements: Vec<PlacementRecord>, published: PublishedResources) -> RetainedEvidence {
    RetainedEvidence::Read {
        placements,
        published: Some(published),
    }
}

/// No publication target: nothing was ever published.
fn no_target(placements: Vec<PlacementRecord>) -> RetainedEvidence {
    RetainedEvidence::Read {
        placements,
        published: None,
    }
}

fn not_retained(result: Result<(), RestoreRefusal>) -> String {
    match result {
        Err(RestoreRefusal::PlacementsNotRetained { detail }) => detail,
        other => panic!("expected not retained, got {other:?}"),
    }
}

#[test]
fn placements_as_published_pass() {
    assert_eq!(
        check(&two_intents(), &read(placed(), published(vec![held(3)], Some(3)))),
        Ok(())
    );
}

#[test]
fn placements_raised_since_publication_pass() {
    assert_eq!(
        check(&two_intents(), &read(placed(), published(vec![held(2)], Some(2)))),
        Ok(())
    );
}

#[test]
fn a_removed_binding_is_refused() {
    let removed = vec![record("acme", "primary", 3, "shared-a")];

    let detail = not_retained(check(
        &two_intents(),
        &read(removed, published(vec![held(3)], Some(3))),
    ));

    assert!(detail.contains("audit"), "{detail}");
}

#[test]
fn a_replaced_binding_masked_by_a_raised_total_is_refused() {
    let replaced = vec![
        record("acme", "primary", 2, "elsewhere"),
        record("acme", "audit", 1, "shared-a"),
    ];

    not_retained(check(
        &two_intents(),
        &read(replaced, published(vec![held(3)], Some(3))),
    ));
}

#[test]
fn a_lowered_binding_is_refused() {
    let lowered = vec![
        record("acme", "primary", 1, "shared-a"),
        record("acme", "audit", 1, "shared-a"),
    ];

    not_retained(check(
        &two_intents(),
        &read(lowered, published(vec![held(3)], Some(3))),
    ));
}

#[test]
fn records_below_the_resource_history_frontier_are_refused() {
    let detail = not_retained(check(
        &two_intents(),
        &read(placed(), published(vec![held(3)], Some(5))),
    ));

    assert!(detail.contains("resource history reached revision 5"), "{detail}");
}

#[test]
fn a_tenant_published_once_and_no_longer_held_is_refused() {
    let detail = not_retained(check(&two_intents(), &read(placed(), published(vec![], Some(3)))));

    assert!(detail.contains("no longer carries it"), "{detail}");
}

#[test]
fn a_placement_for_a_logical_source_no_longer_declared_is_refused() {
    let only_primary = client("  data:\n    primary:\n      class: dedicated\n");

    assert_eq!(
        not_retained(check(
            &only_primary,
            &read(placed(), published(vec![held(3)], Some(3)))
        )),
        "a placement records audit, which the client no longer declares"
    );
}

#[test]
fn unreadable_evidence_is_refused() {
    assert_eq!(
        check(&two_intents(), &RetainedEvidence::Unreadable),
        Err(RestoreRefusal::PlacementsUnreadable)
    );
}

#[test]
fn never_published_passes_only_when_the_targets_history_establishes_it() {
    let mut other = held(4);
    other.tenant = TenantId::try_new("globex").unwrap();
    let elsewhere = || published(vec![other.clone()], None);

    assert_eq!(check(&two_intents(), &read(placed(), elsewhere())), Ok(()));
    assert_eq!(check(&two_intents(), &read(vec![], elsewhere())), Ok(()));
}

#[test]
fn a_target_holding_no_accepted_publication_is_unknown_evidence() {
    assert_eq!(
        check(&two_intents(), &read(placed(), published(vec![], None))),
        Err(RestoreRefusal::PlacementsUnverified)
    );
}

#[test]
fn a_deployment_without_a_platform_or_target_has_only_the_declaration_check() {
    assert_eq!(check(&two_intents(), &RetainedEvidence::NotManaged), Ok(()));
    assert_eq!(check(&two_intents(), &no_target(placed())), Ok(()));
    let only_primary = client("  data:\n    primary:\n      class: dedicated\n");
    assert!(check(&only_primary, &no_target(placed())).is_err());
}
