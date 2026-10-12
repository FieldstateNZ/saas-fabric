//! The retained-placement check a restore passes, rule by rule.

use std::collections::BTreeMap;

use fabric_client_model::{Client, ClientDocument};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{IsolationModelDocument, PlacementRecord, PublishedTenants};

use super::facts::{Placements, PlatformFacts, Publication};
use super::retained::check;
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

fn record(tenant: &str, logical: &str, revision: u64) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).unwrap(),
        logical: LogicalDataSourceName::try_new(logical).unwrap(),
        revision: BindingRevision::new(revision),
        data_source: DataSourceId::try_new("shared-a").unwrap(),
        isolation: IsolationModelDocument::Database {},
        placed_at: "2026-10-11T00:00:00Z".to_owned(),
    }
}

fn published_at(revision: u64) -> Publication {
    Publication::Observed {
        last_pass: None,
        published: Some(PublishedTenants {
            at_unix_seconds: 1,
            tenants: BTreeMap::from([(TenantId::try_new("acme").unwrap(), BindingRevision::new(revision))]),
        }),
    }
}

fn facts(placements: Vec<PlacementRecord>, publication: Publication) -> PlatformFacts {
    PlatformFacts {
        placements: Placements::Read(placements),
        publication,
    }
}

fn placed_twice() -> Vec<PlacementRecord> {
    vec![
        record("acme", "primary", 2),
        record("acme", "audit", 1),
        record("globex", "primary", 9),
    ]
}

#[test]
fn placements_retained_at_the_published_revision_pass() {
    assert_eq!(
        check(&two_intents(), &facts(placed_twice(), published_at(3))),
        Ok(())
    );
}

#[test]
fn placements_raised_since_publication_pass() {
    assert_eq!(
        check(&two_intents(), &facts(placed_twice(), published_at(2))),
        Ok(())
    );
}

#[test]
fn a_removed_placement_is_refused() {
    let removed = vec![record("acme", "primary", 2)];

    let refusal = check(&two_intents(), &facts(removed, published_at(3))).unwrap_err();

    let RestoreRefusal::PlacementsNotRetained { detail } = refusal else {
        panic!("expected not retained, got {refusal:?}");
    };
    assert!(detail.contains("last published at revision 3"), "{detail}");
    assert!(detail.contains("add up to 2"), "{detail}");
}

#[test]
fn a_lowered_placement_is_refused() {
    let lowered = vec![record("acme", "primary", 1), record("acme", "audit", 1)];

    assert!(matches!(
        check(&two_intents(), &facts(lowered, published_at(3))),
        Err(RestoreRefusal::PlacementsNotRetained { .. })
    ));
}

#[test]
fn a_placement_for_a_logical_source_no_longer_declared_is_refused() {
    let only_primary = client("  data:\n    primary:\n      class: dedicated\n");

    let refusal = check(&only_primary, &facts(placed_twice(), published_at(3))).unwrap_err();

    assert_eq!(
        refusal,
        RestoreRefusal::PlacementsNotRetained {
            detail: "a placement records audit, which the client no longer declares".to_owned()
        }
    );
}

#[test]
fn unreadable_placements_are_refused() {
    let unreadable = PlatformFacts {
        placements: Placements::Unreadable,
        publication: published_at(3),
    };

    assert_eq!(
        check(&two_intents(), &unreadable),
        Err(RestoreRefusal::PlacementsUnreadable)
    );
}

#[test]
fn no_publication_since_start_is_refused_rather_than_assumed() {
    let not_yet = Publication::Observed {
        last_pass: None,
        published: None,
    };

    assert_eq!(
        check(&two_intents(), &facts(placed_twice(), not_yet)),
        Err(RestoreRefusal::PlacementsUnverified)
    );
}

#[test]
fn a_tenant_never_published_or_never_placed_passes() {
    let elsewhere = Publication::Observed {
        last_pass: None,
        published: Some(PublishedTenants {
            at_unix_seconds: 1,
            tenants: BTreeMap::new(),
        }),
    };

    assert_eq!(check(&two_intents(), &facts(placed_twice(), elsewhere)), Ok(()));
    assert_eq!(check(&two_intents(), &facts(vec![], published_at(0))), Ok(()));
}

#[test]
fn a_deployment_without_a_platform_or_publisher_has_only_the_declaration_check() {
    assert_eq!(check(&two_intents(), &PlatformFacts::not_managed()), Ok(()));
    assert_eq!(
        check(&two_intents(), &facts(placed_twice(), Publication::NotConfigured)),
        Ok(())
    );
    let only_primary = client("  data:\n    primary:\n      class: dedicated\n");
    assert!(check(&only_primary, &facts(placed_twice(), Publication::NotConfigured)).is_err());
}
