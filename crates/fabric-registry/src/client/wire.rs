//! The response shapes this adapter reads.
//!
//! # Why none of these refuse unknown fields
//!
//! Every shape here is a third party's wire format — the distribution and
//! image specifications, as each registry writes them — which this adapter
//! only reads. It takes what it needs and ignores the rest; refusing a field
//! a registry added would be refusing the registry.

use std::collections::BTreeMap;

mod responses;

pub(super) use responses::{Errors, PullToken, TagList};

/// A manifest or an index, in as much detail as this adapter needs.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Manifest {
    /// What it says it is.
    #[serde(default)]
    pub(super) media_type: Option<String>,

    /// What kind of artifact it is, on an OCI artifact.
    #[serde(default)]
    pub(super) artifact_type: Option<String>,

    /// Present on an image manifest: where its config blob is.
    #[serde(default)]
    pub(super) config: Option<Descriptor>,

    /// Present on an image manifest: its layers.
    #[serde(default)]
    pub(super) layers: Option<Vec<Descriptor>>,

    /// Present on an index: the manifests it points at.
    #[serde(default)]
    pub(super) manifests: Option<Vec<Descriptor>>,

    /// Present on an artifact attached to another: what it is attached to.
    #[serde(default)]
    pub(super) subject: Option<Descriptor>,

    /// Its annotations, by name.
    #[serde(default)]
    pub(super) annotations: Option<BTreeMap<String, String>>,
}

/// An OCI descriptor: a reference to another object, as a config, a layer,
/// an index entry, a subject or a referrer.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Descriptor {
    /// What the object is.
    #[serde(default)]
    pub(super) media_type: Option<String>,

    /// Its digest.
    pub(super) digest: String,

    /// Its length in bytes.
    #[serde(default)]
    pub(super) size: Option<u64>,

    /// On a referrer or an index entry: the artifact type it points at.
    #[serde(default)]
    pub(super) artifact_type: Option<String>,

    /// On an index entry: which platform it is for.
    #[serde(default)]
    pub(super) platform: Option<Platform>,
}

/// An index entry's platform.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct Platform {
    /// `linux`, and so on.
    pub(super) os: String,

    /// `amd64`, and so on.
    pub(super) architecture: String,
}

/// An image config blob, in as much detail as this adapter needs.
#[derive(Debug, Default, serde::Deserialize)]
pub(super) struct Config {
    /// The inner `config` object, which is where labels live.
    #[serde(default)]
    pub(super) config: Option<Labels>,
}

/// The labels baked into an image at build time.
#[derive(Debug, Default, serde::Deserialize)]
pub(super) struct Labels {
    /// Labels, by name. `null` on an image built with none.
    #[serde(rename = "Labels", default)]
    pub(super) labels: Option<BTreeMap<String, String>>,
}
