//! This repository's own `component.yaml` renders, reads back, and agrees
//! with the release workflow's build matrix (ADR 0026 section 10).
//!
//! # Why this runs on every pull request
//!
//! The roles and repositories in `component.yaml` are the contract with
//! `saas-fabric-platform`, which pins these images by these roles, and the
//! release's matrix is what actually builds and pushes them. A role renamed
//! in one and not the other, or a repository that is not the one pushed,
//! would publish a component descriptor naming images that do not exist --
//! discovered only at release, by a reader answering *invalid*. Here it is
//! a failing test instead.
// Helpers outside a `#[test]` function are not covered by clippy.toml's
// allow-in-tests settings; this whole file is a test.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use fabric_component::{ComponentDescriptor, ComponentSource, Digest, MAX_DOCUMENT_BYTES};
use serde_norway::Value;
use std::collections::{BTreeMap, BTreeSet};

const COMPONENT: &str = include_str!("../../../component.yaml");
const RELEASE: &str = include_str!("../../../.github/workflows/release.yml");

fn workflow() -> Value {
    serde_norway::from_str(RELEASE).unwrap()
}

/// The namespace every image of this repository is pushed to, as the
/// workflow builds it: `${REGISTRY}/${GITHUB_REPOSITORY_OWNER,,}/`.
///
/// # Why it is read, not written here
///
/// A registry or owner written into this test would stay green while the
/// workflow pushed somewhere else. The registry is the workflow's own
/// `env.REGISTRY`; the owner is the one in the workspace's `repository`,
/// the GitHub repository the workflow runs in, lower-cased as the workflow
/// lower-cases it.
fn namespace() -> String {
    let registry = workflow()["env"]["REGISTRY"]
        .as_str()
        .expect("the release workflow names its REGISTRY")
        .to_owned();
    let owner = env!("CARGO_PKG_REPOSITORY")
        .strip_prefix("https://github.com/")
        .and_then(|path| path.split('/').next())
        .expect("the workspace's repository is a GitHub repository")
        .to_lowercase();
    format!("{registry}/{owner}/")
}

/// The build matrix's entries: `(role, image, primary)`.
fn matrix() -> Vec<(String, String, bool)> {
    let workflow = workflow();
    let include = &workflow["jobs"]["build"]["strategy"]["matrix"]["include"];
    include
        .as_sequence()
        .expect("the build matrix has include entries")
        .iter()
        .map(|entry| {
            let text = |key: &str| {
                entry[key]
                    .as_str()
                    .unwrap_or_else(|| panic!("a matrix entry has no {key}"))
                    .to_owned()
            };
            let primary = entry["primary"]
                .as_bool()
                .expect("a matrix entry says whether it is primary");
            (text("role"), text("image"), primary)
        })
        .collect()
}

fn source() -> ComponentSource {
    ComponentSource::from_yaml(COMPONENT).unwrap()
}

#[test]
fn the_roles_are_the_matrixs_roles() {
    let source = source();
    let declared: BTreeSet<&str> = source.images().map(|(role, _)| role.as_str()).collect();

    let built: BTreeSet<String> = matrix().into_iter().map(|(role, _, _)| role).collect();

    assert_eq!(declared, built.iter().map(String::as_str).collect());
}

#[test]
fn each_roles_repository_is_the_image_the_matrix_pushes() {
    let source = source();
    let repositories: BTreeMap<&str, &str> = source
        .images()
        .map(|(role, repository)| (role.as_str(), repository.as_str()))
        .collect();

    let namespace = namespace();
    assert_eq!(namespace, "ghcr.io/fieldstatenz/");

    for (role, image, _) in matrix() {
        assert_eq!(
            repositories.get(role.as_str()).copied(),
            Some(format!("{namespace}{image}").as_str()),
            "{role}"
        );
    }
}

#[test]
fn exactly_one_image_is_primary_and_it_is_the_runtime() {
    let primaries: Vec<String> = matrix()
        .into_iter()
        .filter(|(_, _, primary)| *primary)
        .map(|(role, _, _)| role)
        .collect();

    assert_eq!(primaries, ["runtime"]);
}

#[test]
fn it_renders_within_bounds_and_reads_back() {
    let source = source();
    let digest = Digest::try_new(format!("sha256:{}", "ab".repeat(32))).unwrap();
    let digests = source
        .images()
        .map(|(role, _)| (role.clone(), digest.clone()))
        .collect();

    let descriptor = source.render("0.3.0-preview.15", &digests).unwrap();
    let bytes = descriptor.to_json();

    assert!(bytes.len() <= MAX_DOCUMENT_BYTES, "{}", bytes.len());
    assert_eq!(ComponentDescriptor::from_json(&bytes).unwrap(), descriptor);
    assert!(descriptor.spec().capabilities.is_empty());
    assert!(descriptor.spec().fields.is_empty());
    assert!(descriptor.spec().resources.is_empty());
}
