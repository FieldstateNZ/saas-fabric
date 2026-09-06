//! Which specification versions this client will talk to, and why the answer
//! is a floor rather than a minor match.

use super::version::{check_version, VersionOutcome};
use crate::NDC_MINIMUM_VERSION;

// -- Below the floor -> rejected -------------------------------------------

#[test]
fn a_connector_predating_request_level_arguments_is_rejected() {
    // The defect this floor exists for. 0.2.0 has never heard of
    // `request_arguments`, and `ndc-models` carries no `deny_unknown_fields`
    // at any version, so such a connector does not reject the field -- it
    // ignores it, and every tenant lands on the connection it was started
    // with, at `200`.
    let error = check_version("postgres", "0.2.0").unwrap_err();

    assert!(error.contains("0.2.0"), "{error}");
    assert!(error.contains(NDC_MINIMUM_VERSION), "{error}");
    assert!(error.contains("request-level arguments"), "{error}");
}

#[test]
fn every_patch_below_the_floor_is_rejected() {
    for version in ["0.2.0", "0.2.1", "0.2.2", "0.2.3"] {
        assert!(
            check_version("postgres", version).is_err(),
            "{version} was accepted"
        );
    }
}

// -- At the floor -> accepted ----------------------------------------------

#[test]
fn the_floor_itself_is_an_exact_match() {
    // `ndc-postgres` v3.1.0 pins `ndc-models` at v0.2.4 and reports this.
    // Accepting it is the point of choosing 0.2.4 over 0.2.13.
    assert_eq!(
        check_version("postgres", NDC_MINIMUM_VERSION),
        Ok(VersionOutcome::Matched)
    );
}

// -- Above the floor -> accepted, and reported -----------------------------

#[test]
fn a_connector_ahead_of_the_floor_is_accepted_as_a_reportable_difference() {
    // `AheadOfFloor` carrying the connector's version is what
    // `build_ndc_connector` matches on to log the drift -- asserting the
    // variant is what proves the warning path is reached, independent of
    // asserting on a tracing sink.
    assert_eq!(
        check_version("postgres", "0.2.13"),
        Ok(VersionOutcome::AheadOfFloor {
            connector_version: "0.2.13".to_owned()
        })
    );
}

#[test]
fn a_connector_far_ahead_on_the_same_minor_is_still_accepted() {
    // Refusing to start over a patch bump would make every connector upgrade
    // a coordinated release, and additions at patch level are gated behind
    // capabilities we do not claim.
    assert!(matches!(
        check_version("postgres", "0.2.99"),
        Ok(VersionOutcome::AheadOfFloor { .. })
    ));
}

// -- Different minor or major -> rejected in either direction ---------------

#[test]
fn a_newer_minor_version_is_rejected() {
    let error = check_version("postgres", "0.3.0").unwrap_err();

    assert!(error.contains("0.3.0"), "{error}");
}

#[test]
fn an_older_minor_version_is_rejected() {
    let error = check_version("postgres", "0.1.9").unwrap_err();

    assert!(error.contains("0.1.9"), "{error}");
}

#[test]
fn a_newer_major_version_is_rejected() {
    assert!(check_version("postgres", "1.0.0").is_err());
}

#[test]
fn a_newer_major_version_is_rejected_even_if_the_minor_number_coincides() {
    // Major takes precedence: "1.2" is not "compatible enough" just because
    // its minor digit happens to equal ours.
    assert!(check_version("postgres", "1.2.13").is_err());
}

// -- Malformed / unparseable version -> rejected, never silently accepted ---

#[test]
fn an_unparseable_version_does_not_pass_silently() {
    assert!(check_version("postgres", "experimental").is_err());
}

#[test]
fn an_empty_version_does_not_pass_silently() {
    assert!(check_version("postgres", "").is_err());
}

#[test]
fn a_version_with_a_non_numeric_minor_does_not_pass_silently() {
    assert!(check_version("postgres", "0.x.13").is_err());
}

#[test]
fn a_version_with_only_a_major_component_does_not_pass_silently() {
    assert!(check_version("postgres", "2").is_err());
}

#[test]
fn a_version_with_no_patch_component_does_not_pass_silently() {
    // Previously this compared equal on `major.minor` and was accepted. With
    // a patch-level floor there is nothing to compare it against.
    assert!(check_version("postgres", "0.2").is_err());
}

// -- Against the real connector's own capabilities document -----------------

#[test]
fn the_real_connector_reports_the_version_floor_and_it_matches() {
    // `ghcr.io/hasura/ndc-postgres:v3.1.0`'s `GET /capabilities` reports
    // exactly `0.2.4` -- the floor this client requires, not merely a
    // version above it. See `tests/fixtures/ndc-postgres-v3.1.0/README.md`.
    let path = format!(
        "{}/tests/fixtures/ndc-postgres-v3.1.0/capabilities.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let capabilities = std::fs::read_to_string(path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&capabilities).unwrap();
    let reported = parsed["version"].as_str().unwrap();

    assert_eq!(check_version("postgres", reported), Ok(VersionOutcome::Matched));
}
