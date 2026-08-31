//! The response shapes this adapter reads.

/// What `GET /v2/<name>/tags/list` answers with.
#[derive(Debug, serde::Deserialize)]
pub(super) struct TagList {
    /// The tags on this page. Absent rather than empty on some registries.
    #[serde(default)]
    pub(super) tags: Option<Vec<String>>,
}

/// What the token endpoint answers with.
#[derive(Debug, serde::Deserialize)]
pub(super) struct PullToken {
    /// The bearer to present.
    pub(super) token: String,
}

/// A manifest, in as much detail as this adapter needs.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Manifest {
    /// Present on an image manifest: where its config blob is.
    #[serde(default)]
    pub(super) config: Option<Descriptor>,

    /// Present on an index: the per-platform manifests it points at.
    #[serde(default)]
    pub(super) manifests: Option<Vec<PlatformManifest>>,
}

/// A reference to another object.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Descriptor {
    /// Its digest.
    pub(super) digest: String,
}

/// One entry in an index.
#[derive(Debug, serde::Deserialize)]
pub(super) struct PlatformManifest {
    /// The manifest's digest.
    pub(super) digest: String,

    /// Which platform it is for.
    #[serde(default)]
    pub(super) platform: Option<Platform>,
}

/// An index entry's platform.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Platform {
    /// `linux`, and so on.
    pub(super) os: String,

    /// `amd64`, and so on.
    pub(super) architecture: String,
}

/// An image config blob, in as much detail as this adapter needs.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Config {
    /// The inner `config` object, which is where labels live.
    #[serde(default)]
    pub(super) config: Option<Labels>,
}

/// The labels baked into an image at build time.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Labels {
    /// OCI annotations, by name.
    #[serde(rename = "Labels", default)]
    pub(super) labels: Option<std::collections::BTreeMap<String, String>>,
}
