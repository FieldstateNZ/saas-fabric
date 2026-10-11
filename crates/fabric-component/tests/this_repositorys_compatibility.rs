//! This repository's `compatibility.yaml` states the floor and the contracts
//! the binaries are built with (D01-21, #124).
//!
//! The file is what a reviewer and the release read, and the constants are
//! what the binaries enforce. A floor raised in one and not the other would
//! either refuse a rollback the manifest promises or admit one it forbids,
//! so this runs on every pull request.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fabric_component::{ComponentVersion, MINIMUM_ROLLABLE_VERSION, RUNTIME_REPOSITORY};
use fabric_runtime_publication::{CONTRACT_VERSION, MINIMUM_READER_CONTRACT};
use serde_norway::Value;

const COMPATIBILITY: &str = include_str!("../../../compatibility.yaml");
const COMPONENT: &str = include_str!("../../../component.yaml");

/// The last release cut before the publication record (#136), resource
/// history (#138) and client lifecycle (#137) existed. Every release up to it
/// misreads state the current source writes.
const LAST_RELEASE_BEFORE_THE_FORMATS: &str = "0.3.0-preview.15";

fn spec() -> Value {
    let manifest: Value = serde_norway::from_str(COMPATIBILITY).unwrap();
    assert_eq!(manifest["apiVersion"], "fabric.fieldstate.nz/v1");
    assert_eq!(manifest["kind"], "Compatibility");
    manifest["spec"].clone()
}

fn text(value: &Value) -> &str {
    value.as_str().expect("a string")
}

/// `(major, minor, patch)` and the `preview.N` number, for this repository's
/// own version shape. Prerelease precedence is compared numerically, as `SemVer`
/// requires: `preview.9` precedes `preview.16`.
fn preview(version: &str) -> ((u64, u64, u64), u64) {
    let (core, pre) = version.split_once("-preview.").expect("a preview version");
    let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap());
    let core = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    (core, pre.parse().unwrap())
}

#[test]
fn the_floor_is_the_one_the_binaries_enforce() {
    assert_eq!(text(&spec()["minimumRollableVersion"]), MINIMUM_ROLLABLE_VERSION);
}

#[test]
fn the_floor_is_a_version_a_release_can_carry() {
    ComponentVersion::try_new(MINIMUM_ROLLABLE_VERSION).unwrap();
}

#[test]
fn the_floor_is_above_every_release_that_misreads_the_current_formats() {
    assert!(preview(MINIMUM_ROLLABLE_VERSION) > preview(LAST_RELEASE_BEFORE_THE_FORMATS));
}

#[test]
fn the_floor_is_on_the_line_this_workspace_releases() {
    let (core, _) = preview(MINIMUM_ROLLABLE_VERSION);
    let workspace = env!("CARGO_PKG_VERSION");
    assert_eq!(format!("{}.{}.{}", core.0, core.1, core.2), workspace);
}

#[test]
fn the_floor_applies_to_the_runtime_image_component_yaml_names() {
    let spec = spec();
    let component: Value = serde_norway::from_str(COMPONENT).unwrap();

    assert_eq!(spec["component"], component["spec"]["name"]);
    assert_eq!(text(&spec["repository"]), RUNTIME_REPOSITORY);
    assert_eq!(
        text(&component["spec"]["images"]["runtime"]["repository"]),
        RUNTIME_REPOSITORY
    );
}

#[test]
fn the_publication_floor_is_the_publishers_contract_and_readers_are_not_raised() {
    let publication = &spec()["publication"];

    assert_eq!(
        publication["publisherContract"].as_u64(),
        Some(u64::from(CONTRACT_VERSION))
    );
    assert_eq!(
        publication["minimumReaderContract"].as_u64(),
        Some(u64::from(MINIMUM_READER_CONTRACT))
    );
}
