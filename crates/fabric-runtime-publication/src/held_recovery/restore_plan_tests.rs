//! A restore puts back only the exact bytes the held record names, at the
//! record's revision, when they are usable alone and against every usable
//! peer, and never over readable metadata or content it does not name.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use super::super::fixtures::{
    data_source, data_sources, held_naming, manifest, registry, tenant, tenants, torn_tenants, TORN,
};
use super::*;
use crate::{data_sources_canonical_json, tenants_canonical_json, CONTRACT_VERSION};

fn refusal(held: &HeldObservation, document: DocumentKind, offered: &[u8]) -> RestoreRefusal {
    plan_restore(held, document, offered).unwrap_err()
}

#[test]
fn a_torn_payload_is_restored_from_the_exact_recorded_bytes_at_the_recorded_revision() {
    let plan = plan_restore(&torn_tenants(), DocumentKind::Tenants, &tenants()).unwrap();

    assert_eq!(plan.revision, DocumentRevision::new(3));
    assert_eq!(plan.generation, 7);
    assert_eq!(plan.payload, Some(tenants()));
    assert_eq!(
        plan.manifest, None,
        "a valid manifest at the recorded revision is kept"
    );
    assert_eq!(plan.held_payload_sha256, Some(sha256_hex(TORN)));
    assert_eq!(
        plan.record_sha256,
        sha256_hex(torn_tenants().observation.record.as_deref().unwrap())
    );
    assert!(plan.peers.is_empty());
}

#[test]
fn a_lost_payload_is_restored_under_its_held_manifest() {
    let mut held = torn_tenants();
    held.observation.tenants.payload = None;

    let plan = plan_restore(&held, DocumentKind::Tenants, &tenants()).unwrap();

    assert_eq!(plan.payload, Some(tenants()));
    assert_eq!(plan.manifest, None);
}

#[test]
fn only_a_missing_or_malformed_manifest_is_rebuilt_and_only_at_the_recorded_revision() {
    for manifest in [None, Some(b"{\"document\":\"tena".to_vec()), Some(b"[]".to_vec())] {
        let mut held = torn_tenants();
        held.observation.tenants.manifest = manifest;

        let plan = plan_restore(&held, DocumentKind::Tenants, &tenants()).unwrap();

        assert_eq!(
            plan.manifest,
            Some(DocumentManifest::new(
                DocumentKind::Tenants,
                DocumentRevision::new(3)
            ))
        );
    }
}

#[test]
fn a_readable_manifest_of_another_kind_is_refused_never_rebuilt() {
    // Review 5489986163's probe: a readable data-sources manifest at
    // revision 99 sat beside the tenants payload, and the restore rewrote
    // it at revision 3.
    let mut held = torn_tenants();
    held.observation.tenants.manifest = Some(manifest(DocumentKind::DataSources, 99));

    assert!(matches!(
        refusal(&held, DocumentKind::Tenants, &tenants()),
        RestoreRefusal::ManifestConflicts { .. }
    ));
}

#[test]
fn a_manifest_of_an_unknown_shape_or_a_newer_contract_is_refused() {
    let mut unknown: serde_json::Value = serde_json::from_slice(&manifest(DocumentKind::Tenants, 3)).unwrap();
    unknown["signed_by"] = "someone".into();
    let mut newer = unknown.clone();
    newer.as_object_mut().unwrap().remove("signed_by");
    newer["contract_version"] = (CONTRACT_VERSION + 1).into();
    for manifest in [unknown, newer] {
        let mut held = torn_tenants();
        held.observation.tenants.manifest = Some(serde_json::to_vec(&manifest).unwrap());

        assert!(matches!(
            refusal(&held, DocumentKind::Tenants, &tenants()),
            RestoreRefusal::ManifestConflicts { .. }
        ));
    }
}

#[test]
fn a_readable_manifest_at_another_revision_is_refused_ahead_or_behind() {
    for revision in [2, 4, 99] {
        let mut held = torn_tenants();
        held.observation.tenants.manifest = Some(manifest(DocumentKind::Tenants, revision));

        assert_eq!(
            refusal(&held, DocumentKind::Tenants, &tenants()),
            RestoreRefusal::ManifestDisagrees {
                document: DocumentKind::Tenants,
                manifest: DocumentRevision::new(revision),
                recorded: DocumentRevision::new(3),
            }
        );
    }
}

#[test]
fn semantically_equal_bytes_that_are_not_the_recorded_bytes_are_refused() {
    let pretty =
        serde_json::to_vec_pretty(&serde_json::from_slice::<serde_json::Value>(&tenants()).unwrap()).unwrap();

    assert!(matches!(
        refusal(&torn_tenants(), DocumentKind::Tenants, &pretty),
        RestoreRefusal::DigestMismatch { .. }
    ));
}

#[test]
fn recorded_duplicate_tenants_with_no_data_are_refused() {
    // Review 5489986163's probe: two acme tenants with empty data maps,
    // named by the record, received a write plan.
    let mut acme: serde_json::Value = serde_json::from_slice(&tenants()).unwrap();
    acme[0]["data"] = serde_json::json!({});
    let duplicated = serde_json::to_vec(&serde_json::json!([acme[0], acme[0]])).unwrap();
    let mut held = held_naming(&duplicated, &data_sources(), None);
    held.observation.tenants.payload = Some(TORN.to_vec());

    let RestoreRefusal::OfferedInvalid { problems, .. } = refusal(&held, DocumentKind::Tenants, &duplicated)
    else {
        panic!("expected the offered tenants to be refused as invalid");
    };
    assert!(problems.iter().any(|problem| problem.contains("more than once")));
    assert!(problems.iter().any(|problem| problem.contains("no data binding")));
}

#[test]
fn recorded_tenants_bound_to_an_undeclared_data_source_are_refused() {
    let dangling = tenants_canonical_json(&[tenant("acme", "gone")]).unwrap();
    let mut held = held_naming(&dangling, &data_sources(), None);
    held.observation.tenants.payload = Some(TORN.to_vec());

    assert!(matches!(
        refusal(&held, DocumentKind::Tenants, &dangling),
        RestoreRefusal::OfferedInvalid { .. }
    ));
}

#[test]
fn recorded_data_sources_that_drop_one_a_held_tenant_binds_are_refused() {
    let other = data_sources_canonical_json(&[data_source("other"), data_source("other")]).unwrap();
    let mut held = held_naming(&tenants(), &other, None);
    held.observation.data_sources.payload = Some(TORN.to_vec());

    let RestoreRefusal::OfferedInvalid { problems, .. } = refusal(&held, DocumentKind::DataSources, &other)
    else {
        panic!("expected the offered data sources to be refused as invalid");
    };
    assert!(problems
        .iter()
        .any(|problem| problem.contains("declared more than once")));
    assert!(problems.iter().any(|problem| problem.contains("not declared")));
}

#[test]
fn recorded_tenants_that_leave_a_registered_issuer_unbound_are_refused() {
    let globex = registry("globex");
    let mut held = held_naming(&tenants(), &data_sources(), Some(&globex));
    held.observation.tenants.payload = Some(TORN.to_vec());

    assert!(matches!(
        refusal(&held, DocumentKind::Tenants, &tenants()),
        RestoreRefusal::OfferedInvalid { .. }
    ));
}

#[test]
fn a_damaged_peer_is_reported_and_does_not_stand_in_for_a_check() {
    let acme = registry("acme");
    let mut held = held_naming(&tenants(), &data_sources(), Some(&acme));
    held.observation.tenants.payload = Some(TORN.to_vec());
    held.observation.data_sources.payload = Some(b"[".to_vec());
    held.observation.registry = Some(b"{}".to_vec());

    let plan = plan_restore(&held, DocumentKind::Tenants, &tenants()).unwrap();

    let parts: Vec<Part> = plan.peers.iter().map(|peer| peer.part).collect();
    assert!(parts.contains(&Part::DataSources), "{:?}", plan.peers);
    assert!(parts.contains(&Part::IssuerRegistry), "{:?}", plan.peers);
    assert!(!parts.contains(&Part::Tenants));
}

#[test]
fn readable_held_content_the_record_does_not_name_is_never_replaced() {
    let mut held = torn_tenants();
    held.observation.tenants.payload = Some(b"[]".to_vec());

    assert_eq!(
        refusal(&held, DocumentKind::Tenants, &tenants()),
        RestoreRefusal::HeldReadable {
            document: DocumentKind::Tenants
        }
    );
}

#[test]
fn bytes_already_restored_are_a_no_op() {
    let held = held_naming(&tenants(), &data_sources(), None);

    let plan = plan_restore(&held, DocumentKind::Tenants, &tenants()).unwrap();

    assert!(plan.already_restored());
}

#[test]
fn the_catalogue_is_never_restored() {
    assert_eq!(
        refusal(&torn_tenants(), DocumentKind::Catalog, b"{}"),
        RestoreRefusal::NotRestorable {
            document: DocumentKind::Catalog
        }
    );
}

#[test]
fn no_record_an_unreadable_one_or_one_above_this_publishers_floor_is_refused() {
    let mut absent = torn_tenants();
    absent.observation.record = None;
    assert_eq!(
        refusal(&absent, DocumentKind::Tenants, &tenants()),
        RestoreRefusal::NoRecord
    );

    let mut unreadable = torn_tenants();
    unreadable.observation.record = Some(b"{".to_vec());
    assert!(matches!(
        refusal(&unreadable, DocumentKind::Tenants, &tenants()),
        RestoreRefusal::RecordUnusable { .. }
    ));

    let mut above = torn_tenants();
    let mut record: serde_json::Value =
        serde_json::from_slice(above.observation.record.as_deref().unwrap()).unwrap();
    record["minimum_publisher_contract"] = (CONTRACT_VERSION + 1).into();
    above.observation.record = Some(serde_json::to_vec(&record).unwrap());
    assert!(matches!(
        refusal(&above, DocumentKind::Tenants, &tenants()),
        RestoreRefusal::RecordUnusable { detail } if detail.contains("publisher contract")
    ));
}

#[test]
fn an_unstable_observation_is_never_planned_against() {
    let mut held = torn_tenants();
    held.stable = false;

    assert_eq!(
        refusal(&held, DocumentKind::Tenants, &tenants()),
        RestoreRefusal::Unstable
    );
}

#[test]
fn recorded_bytes_that_do_not_parse_are_refused() {
    let mut held = held_naming(b"{}", &data_sources(), None);
    held.observation.tenants.payload = Some(TORN.to_vec());

    assert!(matches!(
        refusal(&held, DocumentKind::Tenants, b"{}"),
        RestoreRefusal::OfferedUnparseable { .. }
    ));
}

#[test]
fn the_plan_reports_what_it_writes_and_never_the_bytes() {
    let plan = plan_restore(&torn_tenants(), DocumentKind::Tenants, &tenants()).unwrap();
    let report = serde_json::to_string(&plan).unwrap();
    let value: serde_json::Value = serde_json::from_str(&report).unwrap();

    assert_eq!(value["writes_payload"], true);
    assert_eq!(value["writes_manifest"], serde_json::Value::Null);
    assert_eq!(value["revision"], 3);
    assert!(!report.contains("acme"));
}
