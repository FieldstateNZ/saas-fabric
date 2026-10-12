//! A diagnosis is clear only when every recorded part is held, matches the
//! record and is usable, and it never quotes a held value.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use super::super::fixtures::{data_sources, held_naming, registry, tenants, torn_tenants, TORN};
use super::*;
use crate::plan_restore;

const SENTINEL: &str = "SENTINEL-private-7f3a";

fn problems_of(held: &HeldObservation) -> Vec<(Part, ProblemKind)> {
    diagnose(held)
        .problems
        .iter()
        .map(|problem| (problem.part, problem.kind))
        .collect()
}

#[test]
fn a_whole_generation_is_clear() {
    let diagnosis = diagnose(&held_naming(&tenants(), &data_sources(), Some(&registry("acme"))));

    assert!(diagnosis.clear, "{:?}", diagnosis.problems);
    assert_eq!(diagnosis.issuer_registry.held_generation, Some(1));
}

#[test]
fn a_recorded_registry_whose_matching_bytes_are_invalid_is_never_clear() {
    // Review 5489998270's probe: the record named a registry whose digest
    // matched raw bytes that do not parse, and validate exited 0.
    let invalid = b"{\"schemaVersion\":1}";

    let diagnosis = diagnose(&held_naming(&tenants(), &data_sources(), Some(invalid)));

    assert!(!diagnosis.clear);
    assert!(diagnosis
        .problems
        .iter()
        .any(|problem| problem.part == Part::IssuerRegistry));
}

#[test]
fn a_recorded_registry_that_is_missing_or_names_an_unbound_tenant_is_never_clear() {
    let mut missing = held_naming(&tenants(), &data_sources(), Some(&registry("acme")));
    missing.observation.registry = None;
    let unbound = held_naming(&tenants(), &data_sources(), Some(&registry("globex")));

    for held in [missing, unbound] {
        assert!(problems_of(&held).contains(&(Part::IssuerRegistry, ProblemKind::Unusable)));
    }
}

#[test]
fn torn_lost_mismatched_unusable_and_unstable_states_are_each_reported() {
    assert!(problems_of(&torn_tenants()).contains(&(Part::Tenants, ProblemKind::BlocksPublication)));

    let mut lost = torn_tenants();
    lost.observation.tenants.payload = None;
    assert!(problems_of(&lost).contains(&(Part::Tenants, ProblemKind::BlocksPublication)));

    let mut mismatched = held_naming(&tenants(), &data_sources(), None);
    mismatched.observation.tenants.payload = Some(b"[]".to_vec());
    assert!(problems_of(&mismatched).contains(&(Part::Tenants, ProblemKind::Mismatch)));

    let mut both: serde_json::Value = serde_json::from_slice(&tenants()).unwrap();
    let acme = both[0].clone();
    both.as_array_mut().unwrap().push(acme);
    let both = serde_json::to_vec(&both).unwrap();
    assert!(problems_of(&held_naming(&both, &data_sources(), None))
        .contains(&(Part::Tenants, ProblemKind::Unusable)));

    let mut unstable = held_naming(&tenants(), &data_sources(), None);
    unstable.stable = false;
    assert_eq!(
        problems_of(&unstable),
        [(Part::Observation, ProblemKind::Unstable)]
    );
}

#[test]
fn no_held_value_reaches_a_diagnosis_or_a_refusal() {
    // Review 5490101140's probe: an invalid isolation value carrying a
    // private sentinel came back out through serde's message.
    let mut tenants_value: serde_json::Value = serde_json::from_slice(&tenants()).unwrap();
    tenants_value[0]["data"]["primary"]["isolation"] = serde_json::json!({ SENTINEL: {} });
    let leaky_tenants = serde_json::to_vec(&tenants_value).unwrap();
    let leaky_manifest = format!("{{\"document\":\"{SENTINEL}\",\"revision\":3,\"contract_version\":3}}");
    let leaky_registry = format!("{{\"schemaVersion\":\"{SENTINEL}\"}}");

    let mut held = held_naming(&leaky_tenants, &data_sources(), Some(leaky_registry.as_bytes()));
    held.observation.data_sources.manifest = Some(leaky_manifest.into_bytes());
    let mut unreadable_record = held.clone();
    unreadable_record.observation.record = Some(format!("{{\"generation\":\"{SENTINEL}\"}}").into_bytes());

    for held in [&held, &unreadable_record] {
        let report = serde_json::to_string(&diagnose(held)).unwrap();
        assert!(!report.contains(SENTINEL), "{report}");
    }
    let refusal = plan_restore(&held, DocumentKind::Tenants, &leaky_tenants).unwrap_err();
    assert!(!refusal.to_string().contains(SENTINEL), "{refusal}");
    let refusal = plan_restore(&unreadable_record, DocumentKind::Tenants, TORN).unwrap_err();
    assert!(!refusal.to_string().contains(SENTINEL), "{refusal}");
}
