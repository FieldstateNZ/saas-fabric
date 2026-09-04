//! What a component is asked to run, and how one image of it is identified.

/// One image of a component.
///
/// No `pinnedIn`. Where a version is *written* is the component's statement
/// now, not each image's — because a chart has one version and no images, and
/// the two kinds have to answer that question in the same place.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImagePin {
    /// The registry repository this image is published to.
    pub repository: String,

    /// The digest currently asked for.
    pub digest: String,
}

/// What a component is asked to run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Desired {
    /// The version, once.
    ///
    /// Not repeated per image: several images claiming a version separately is
    /// several places for them to disagree, and disagreement is what makes a
    /// release unit incomplete rather than eligible.
    ///
    /// For a chart this is the **chart** version, which is what Argo pins. An
    /// application version is metadata beside it and is never what Fabric
    /// writes.
    pub version: String,
}
