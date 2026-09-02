//! What a component is, and where its version is written.

use std::collections::BTreeMap;

/// Where a component's versions come from.
///
/// # A closed set, and it stays closed
///
/// Two kinds, because the platform has two: images published to a registry,
/// and charts published to a chart repository. A third arrives by adding a
/// variant here and an implementation behind it — not by making this general
/// enough to describe one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Artifact {
    /// Container images, published to a registry.
    ///
    /// Several of them, moving together: SaaS Fabric is one release unit
    /// published as three images, and a version they do not all carry is not a
    /// version this environment can run.
    Oci {
        /// Images by role, e.g. `runtime`, `controlPlane`, `console`.
        images: BTreeMap<String, ImagePin>,
    },
}

/// One image of a component.
///
/// No `pinnedIn`. Where a version is *written* is the component's statement
/// now, not each image's — because a chart has one version and no images, and
/// the two kinds have to answer the same question in the same place.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImagePin {
    /// The registry repository this image is published to.
    pub repository: String,

    /// The digest currently asked for.
    pub digest: String,
}

/// How a version is written into one file.
///
/// # Closed and explicit, deliberately
///
/// This is the field that would ruin the design if it grew a general escape.
/// A `jsonPath`, a regex, a YAML pointer or a shell snippet here would turn a
/// trusted platform document into an arbitrary repository-edit engine, and
/// Fabric into the deputy that runs it — the same mistake as letting a caller
/// name a file, one level further in.
///
/// Every renderer knows exactly what it edits and refuses anything else. A
/// pin somewhere new is a variant here, reviewed like the code it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Renderer {
    /// An `images:` entry in a Kustomize overlay, carrying a tag and a digest.
    KustomizeImage,
}

/// One place a component's version is written.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pin {
    /// The repository-relative file, which must sit under a managed root.
    pub path: String,

    /// How this file carries the version.
    pub renderer: Renderer,

    /// Which image this file pins, for a component that has several.
    ///
    /// Names a key of [`Artifact::Oci::images`]. Absent for an artifact with
    /// one version and no images to tell apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

/// What a component is asked to run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Desired {
    /// The version, once. Not repeated per image: several images claiming a
    /// version separately is several places for them to disagree, and
    /// disagreement is what makes a release unit incomplete rather than
    /// eligible.
    pub version: String,

    /// The commit every image was built from.
    ///
    /// Optional because not every artifact has one. An image carries the
    /// commit it was built from and Fabric checks that they agree; a chart
    /// published by somebody else does not, and inventing a value to fill the
    /// field would be recording something nobody observed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
}
