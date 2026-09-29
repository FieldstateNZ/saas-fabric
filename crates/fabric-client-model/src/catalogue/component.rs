//! What an application is made of: its components, and for a described one
//! the resolution the server recorded when an operator selected it.
use crate::ClientId;
use fabric_component::{ComponentDescriptor, ComponentVersion, Digest, Repository};
use serde::{Deserialize, Serialize};

/// A deployable or platform capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplicationComponent {
    /// Stable key within its application.
    pub id: ClientId,
    /// Display name.
    pub name: String,
    /// Deployment mechanism.
    pub kind: ComponentKind,
    /// OCI image, chart reference, or capability name; for a described
    /// component, its resolution's primary repository.
    pub reference: String,
    /// Pinned artifact version or digest; empty for platform capabilities;
    /// for a described component, its resolution's version tag.
    pub version: String,
    /// Included for every plan.
    pub required: bool,
    /// Automatic or manual advancement intent.
    pub policy: UpdatePolicy,
    /// What the server resolved when this version was selected: present on
    /// a described component and on no other kind. Skipped when absent, so
    /// every document written before ADR 0026 reads and renders byte for
    /// byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<ComponentResolution>,
}

/// Supported component categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    /// Container image.
    Container,
    /// Chart installation.
    Helm,
    /// Platform-provided service.
    Capability,
    /// A component selected by its primary image's repository and a version
    /// tag, and resolved by the server through the component descriptor
    /// attached to that image (ADR 0026 section 7).
    Described,
}

impl ComponentKind {
    /// The kind as it is written, for a message that names one.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Helm => "helm",
            Self::Capability => "capability",
            Self::Described => "described",
        }
    }
}

/// How a deployable may advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePolicy {
    /// Follow the platform's approved releases.
    Automatic,
    /// Require an operator's decision.
    Manual,
}

/// What the server observed when it resolved a described component: the
/// release unit ADR 0026 section 3 called complete, and a frozen copy of its
/// component descriptor.
///
/// # Why a copy, and checked on every read
///
/// A release freezes its definition by copying it (ADR 0021), and what a
/// component declares is part of that definition, so the descriptor is
/// copied whole -- `apiVersion` and `kind` included -- rather than referred
/// to by digest. Every catalogue read and write checks the copy with the
/// rules of the version it records, and checks that it agrees with the rest
/// of the resolution and with its component: the component's `reference` is
/// this `repository` and its `version` this `version`; the descriptor names
/// this `version` and names `repository` at `primary_digest`. A hand edit
/// that breaks any of these makes the whole catalogue unreadable, which is
/// the point: nothing here was typed by an operator, so nothing here may be
/// edited as if it were.
///
/// Only the server writes one: a save cannot carry it (`ApplicationDraft`),
/// and `Catalogue::select_component` is the one way in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComponentResolution {
    /// The primary image's repository, written in full.
    pub repository: Repository,
    /// The version tag the component was selected by.
    pub version: ComponentVersion,
    /// The digest the version tag resolved to, which Fabric computed.
    pub primary_digest: Digest,
    /// The digest of the component descriptor attached to it.
    pub descriptor_digest: Digest,
    /// The one commit every image and the component descriptor name.
    ///
    /// Non-empty text of at most 256 bytes without control characters, not
    /// a 40-character hexadecimal commit: section 3 requires that there be
    /// exactly one revision, not what one looks like, and a repository
    /// using SHA-256 object names has 64-character commits.
    pub revision: String,
    /// When it was resolved, Unix seconds -- what Fabric observed then, not
    /// a claim that the artifact is still there.
    pub resolved_at: u64,
    /// The whole component descriptor, frozen.
    pub descriptor: ComponentDescriptor,
}
