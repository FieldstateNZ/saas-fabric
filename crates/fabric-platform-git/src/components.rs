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

mod document;
mod overlay;
mod pinning;

pub(crate) use document::Document;
pub(crate) use overlay::repin;
pub(crate) use pinning::check_writable;

/// The manifest shape this crate is written against.
///
/// A manifest declaring anything else is refused rather than half-understood:
/// a field that has moved is worse read optimistically than not at all.
pub const SCHEMA_VERSION: u32 = 1;

/// One image of a component, and where its pin is written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePin {
    /// The registry repository this image is published to.
    pub repository: String,

    /// The digest currently asked for.
    pub digest: String,

    /// The files that carry this pin, declared by the platform repository.
    pub pinned_in: Vec<String>,
}

/// What a component is asked to run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Desired {
    /// The version, once. Not repeated per image: three images claiming a
    /// version separately is three places for them to disagree, and
    /// disagreement is what makes a release unit incomplete rather than
    /// eligible.
    pub version: String,

    /// The commit every image was built from.
    pub source_revision: String,

    /// Images by role, e.g. `runtime`, `controlPlane`, `console`.
    pub images: BTreeMap<String, ImagePin>,
}

/// One component's desired state and the policy that moves it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    /// Where to look for newer versions.
    pub channel: Channel,

    /// Whether it advances on its own.
    pub update: UpdatePolicy,

    /// What it is asked to run.
    pub desired: Desired,

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
