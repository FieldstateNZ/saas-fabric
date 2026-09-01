//! What a caller asks a desired-state write to do.

use std::collections::BTreeMap;

/// One image's new identity, as the caller resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDigest {
    /// Where the image was published. Checked against what the manifest
    /// already declares for this role, and refused if it disagrees — a caller
    /// may move a component to a new *version*, never to a new registry.
    pub repository: String,

    /// The immutable digest to deploy.
    pub digest: String,
}

/// What a caller asks a component to move to.
///
/// A release unit: one version, one source commit, and every image the
/// component publishes. Not a per-image update — promoting the console without
/// the control plane would put two thirds of a release on an environment and
/// call it integrated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentVersion {
    /// The version, once.
    pub version: String,

    /// The commit every image was built from.
    pub source_revision: String,

    /// Images by role. Must be exactly the roles the manifest declares.
    pub images: BTreeMap<String, ImageDigest>,
}
