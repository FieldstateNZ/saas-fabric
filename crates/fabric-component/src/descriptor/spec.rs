//! What a component descriptor says: the `spec` inside its envelope.
use super::{ComponentName, ComponentVersion, Digest, Repository, Role};
use crate::{ApplicationResource, ConfigurationField, PlatformCapability};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One image of a component: where it lives and exactly which bytes it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageReference {
    /// The image's repository, written in full.
    pub repository: Repository,
    /// The image's digest -- an index's, for a multi-platform image.
    pub digest: Digest,
}

/// What a component is (ADR 0026 section 2).
///
/// Every field is always serialized, defaults included, and `images` is a
/// sorted map, so one component has one rendering: [`to_json`] is canonical.
/// The sections a document may omit when read are the ones a component may
/// have nothing to say in.
///
/// [`to_json`]: super::ComponentDescriptor::to_json
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentSpec {
    /// The component's name, a DNS label.
    pub name: ComponentName,
    /// What an operator sees it called.
    pub title: String,
    /// What it is, in a sentence or two.
    #[serde(default)]
    pub description: String,
    /// The version this document describes, equal to the tag it was found
    /// by.
    pub version: ComponentVersion,
    /// Every image, by role; all on one registry, at most
    /// [`MAX_IMAGES`](super::MAX_IMAGES).
    pub images: BTreeMap<Role, ImageReference>,
    /// What the software needs from the platform -- needs, never a claim
    /// that anything provides them.
    #[serde(default)]
    pub capabilities: Vec<PlatformCapability>,
    /// Non-secret configuration every client of it supplies.
    #[serde(default)]
    pub fields: Vec<ConfigurationField>,
    /// Data API resources it exposes, by logical names the software uses.
    #[serde(default)]
    pub resources: Vec<ApplicationResource>,
}
