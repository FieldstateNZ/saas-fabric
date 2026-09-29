//! The platform repository's desired-state manifest.
//!
//! `environments/<environment>/components.yaml` says what an environment is
//! *asked* to run. It is desired state and nothing else: what versions are
//! available is discovered from registries, and what is running comes from the
//! cluster, so neither is read or written here.
//!
//! # What this crate is allowed to know
//!
//! Nothing about where a component's version is pinned. The manifest declares
//! that, per image, in `pinnedIn` — so this repository owns its own layout and
//! can move a file without waiting for a Fabric release. When it eventually
//! renders those overlays *from* this manifest instead, the lists empty and
//! this crate writes one file, unchanged.

use std::collections::BTreeMap;

pub use fabric_platform_management::{Channel, Hold, UpdatePolicy};

mod argo;
#[cfg(test)]
#[path = "components/argo_tests.rs"]
mod argo_tests;
mod artifact;
mod document;
mod model;
mod overlay;
mod pin;
mod pinning;
mod schema;

pub use artifact::Artifact;
pub use model::{Desired, ImagePin};
pub use pin::Pin;

pub(crate) use argo::retarget;
pub(crate) use artifact::{DESCRIBED_WORDS, HELM_WORDS, IMAGES_WORDS};
pub(crate) use document::Document;
pub(crate) use overlay::repin;
pub(crate) use pinning::check_writable;

/// The manifest shapes this crate is written against, oldest first.
///
/// A manifest declaring anything else is refused rather than half-understood:
/// a field that has moved is worse read optimistically than not at all.
///
/// # Why two, and why a file keeps its own
///
/// Schema 3 adds one thing, the `described` artifact (ADR 0026 section 9),
/// and a build that reads it has to ship before any file uses it — so it
/// reads a schema 2 file too, and writes back the version it read. A schema 2
/// file stays schema 2 until somebody deliberately moves it: a Fabric release
/// bumping the version under an older build's feet would take that build's
/// whole environment offline, since it refuses the file by version.
pub const SCHEMA_VERSIONS: &[u32] = &[2, DESCRIBED_SCHEMA_VERSION];

/// The first schema a `described` artifact may appear in.
pub const DESCRIBED_SCHEMA_VERSION: u32 = 3;

/// One component's desired state and the policy that moves it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Where this component's versions come from.
    pub artifact: Artifact,

    /// Where to look for newer versions.
    pub channel: Channel,

    /// Whether it advances on its own.
    pub update: UpdatePolicy,

    /// What it is asked to run.
    pub desired: Desired,

    /// Every place this component's version is written.
    ///
    /// The component's statement rather than each image's, because the two
    /// artifact kinds have to answer this in the same place — and because a
    /// reader asking "what does moving this component touch?" should find one
    /// list rather than assembling it.
    pub pinned_in: Vec<Pin>,

    /// Present while advancement is paused.
    pub hold: Option<Hold>,
}

/// An environment's whole desired state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// The shape of this document.
    pub schema_version: u32,

    /// Which environment it describes.
    pub environment: String,

    /// The only directories any `pinnedIn` may point into.
    pub managed_roots: Vec<String>,

    /// Components, by name.
    pub components: BTreeMap<String, Component>,
}
