//! Each readiness condition's evidence, and that missing evidence is never
//! readiness.

use std::collections::BTreeMap;

use fabric_client_model::lifecycle::Evidence;
use fabric_client_model::{Client, ClientDocument, ProjectionCheck};
use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{IsolationModelDocument, PlacementRecord, PublishedTenants};
use fabric_reconciliation::ReconciliationStatus;

use super::evidence::{identity, provisioning, publication};
use super::facts::{Placements, PlatformFacts, Publication};
use crate::continuity::Admission;
use crate::ContinuityHold;

/// What a test says is known about publication.
enum Published {
    NoPublisher,
    NotYet,
    Known(PublishedTenants),
}

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

fn read(placements: Vec<PlacementRecord>, published: Published) -> PlatformFacts {
    let publication = match published {
        Published::NoPublisher => Publication::NotConfigured,
        Published::NotYet => Publication::Observed {
            last_pass: None,
            published: None,
        },
        Published::Known(published) => Publication::Observed {
            last_pass: None,
            published: Some(published),
        },
    };

    PlatformFacts {
        placements: Placements::Read(placements),
        publication,
    }
}

fn known(revision: u64) -> Published {
    Published::Known(PublishedTenants {
        at_unix_seconds: 1,
        tenants: BTreeMap::from([(TenantId::try_new("acme").unwrap(), BindingRevision::new(revision))]),
    })
}

fn is_unmet(evidence: &Evidence) -> bool {
    matches!(evidence, Evidence::Unmet(_))
}

#[test]
fn provisioning_is_met_once_every_declared_intent_is_placed() {
    let facts = read(
        vec![record("acme", "primary", 1), record("acme", "audit", 1)],
        Published::NotYet,
    );

    assert_eq!(provisioning(&two_intents(), &facts), Evidence::Met);
}

#[test]
fn provisioning_names_what_is_not_placed_yet() {
    let facts = read(
        vec![record("acme", "primary", 1), record("globex", "audit", 1)],
        Published::NotYet,
    );

    assert_eq!(
        provisioning(&two_intents(), &facts),
        Evidence::Unmet("not placed yet: audit".to_owned())
    );
}

#[test]
fn a_client_declaring_no_data_is_not_provisioned() {
    assert!(is_unmet(&provisioning(
        &client(""),
        &read(vec![], Published::NotYet)
    )));
}

#[test]
fn no_platform_or_unreadable_placements_is_never_provisioned_or_published() {
    let unreadable = PlatformFacts {
        placements: Placements::Unreadable,
        publication: Publication::NotConfigured,
    };
    for facts in [PlatformFacts::not_managed(), unreadable] {
        assert!(is_unmet(&provisioning(&two_intents(), &facts)));
        assert!(is_unmet(&publication(&two_intents(), &facts)));
    }
}

#[test]
fn identity_is_met_only_when_in_effect_at_this_revision() {
    let consistent = ProjectionCheck::Consistent;
    assert_eq!(
        identity(ReconciliationStatus::Applied, &consistent, Admission::Admitted),
        Evidence::Met
    );
    assert_eq!(
        identity(ReconciliationStatus::Drifted, &consistent, Admission::Admitted),
        Evidence::Met
    );
    assert!(is_unmet(&identity(
        ReconciliationStatus::Pending,
        &consistent,
        Admission::Admitted
    )));
    assert!(is_unmet(&identity(
        ReconciliationStatus::Failed,
        &consistent,
        Admission::Admitted
    )));
    assert!(is_unmet(&identity(
        ReconciliationStatus::HandEdited,
        &consistent,
        Admission::Admitted
    )));
    assert!(is_unmet(&identity(
        ReconciliationStatus::Withheld,
        &consistent,
        Admission::Admitted
    )));
}

#[test]
fn a_projection_that_is_not_consistent_is_never_in_effect_whatever_the_status() {
    let missing = ProjectionCheck::Diverged(vec![fabric_client_model::ProjectedClientDivergence {
        client: fabric_client_model::OidcClientId::try_new("analytics").unwrap(),
        field: fabric_client_model::ProjectedClientField::Missing,
    }]);
    let unverifiable = ProjectionCheck::Unverifiable {
        unverified: Vec::new(),
    };

    for projection in [&missing, &unverifiable] {
        assert!(is_unmet(&identity(
            ReconciliationStatus::Applied,
            projection,
            Admission::Admitted
        )));
        assert!(is_unmet(&identity(
            ReconciliationStatus::Drifted,
            projection,
            Admission::Admitted
        )));
    }
}

#[test]
fn publication_is_met_at_the_revision_the_placements_add_up_to() {
    let facts = read(
        vec![record("acme", "primary", 1), record("acme", "audit", 2)],
        known(3),
    );

    assert_eq!(publication(&two_intents(), &facts), Evidence::Met);
}

#[test]
fn a_placement_newer_than_the_last_publication_leaves_publication_unmet() {
    let facts = read(
        vec![record("acme", "primary", 1), record("acme", "audit", 1)],
        known(1),
    );

    assert_eq!(
        publication(&two_intents(), &facts),
        Evidence::Unmet("published at revision 1, and its placements add up to 2".to_owned())
    );
}

#[test]
fn publication_is_unmet_without_a_publisher_a_pass_or_the_tenant() {
    let placements = || vec![record("acme", "primary", 1)];
    let absent = Published::Known(PublishedTenants {
        at_unix_seconds: 1,
        tenants: BTreeMap::new(),
    });

    for published in [Published::NoPublisher, Published::NotYet, absent] {
        assert!(is_unmet(&publication(
            &two_intents(),
            &read(placements(), published)
        )));
    }
}

#[test]
fn identity_held_by_the_continuity_store_is_never_in_effect_whatever_the_status() {
    for hold in [
        ContinuityHold::NoStore,
        ContinuityHold::Unavailable,
        ContinuityHold::Unenrolled,
        ContinuityHold::Mismatch,
        ContinuityHold::Pending,
        ContinuityHold::Resurrection,
    ] {
        let evidence = identity(
            ReconciliationStatus::Applied,
            &ProjectionCheck::Consistent,
            Admission::Held(hold),
        );
        let Evidence::Unmet(detail) = evidence else {
            panic!("{hold:?} must leave identity unmet");
        };
        assert!(detail.starts_with("the client's identity is held"), "{detail}");
    }
}
