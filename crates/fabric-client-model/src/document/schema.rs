//! The document's outer shape: what identifies it, and how its fields are
//! named.

/// The API version every document this model writes carries.
///
/// Versioned from the start, and checked on read. A future change to the
/// document's shape ships as `v2` alongside this one rather than reinterpreting
/// documents already in the repository — the same policy the Data API applies
/// to its path prefix, for the same reason: the repository holds documents
/// nobody is going to migrate on a schedule the platform controls.
pub const API_VERSION: &str = "fabric.fieldstate.nz/v1";

/// The kind every document this model writes carries.
///
/// `Client`, not `Tenant`. The two name the same organisation from different
/// planes — see this crate's documentation — and the control plane's documents
/// are client-shaped because that is the vocabulary an operator uses.
pub const KIND: &str = "Client";

/// [`API_VERSION`] and [`KIND`] as one string, for the message a rejected
/// document produces.
///
/// Spelled out rather than composed, because the error field it fills is a
/// `&'static str` and neither `format!` nor `concat!` can build one from two
/// consts. The test below is what keeps it honest.
pub(super) const EXPECTED_DOCUMENT: &str = "fabric.fieldstate.nz/v1/Client";

/// The document, exactly as it is deserialised.
///
/// Private to this module and deliberately narrow: it is the *reading* shape,
/// used to derive the typed view. Writing goes through the preserved raw
/// document instead, which is why this struct does not need — and must not
/// have — a field for every section a real document holds.
///
/// `apiVersion` and `kind` are absent because they are checked *before* this
/// shape is deserialised — see `parse::check_document_kind` for why that order
/// matters. Declaring them here as well would add two fields nothing reads.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DocumentShape {
    /// Identity of the resource itself.
    pub(super) metadata: MetadataShape,

    /// The declared desired state.
    pub(super) spec: SpecShape,
}

/// The `metadata` block.
#[derive(serde::Deserialize)]
pub(super) struct MetadataShape {
    /// The client id, which is also the document's directory name.
    pub(super) name: crate::ClientId,
}

/// The parts of `spec` this model understands.
///
/// **No `deny_unknown_fields`, deliberately.** Unknown sections are the normal
/// case — features, data, configuration — and refusing them would make this
/// model unable to read any document richer than itself.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SpecShape {
    /// The name an operator sees.
    pub(super) display_name: String,

    /// The hostnames the client is reached on.
    #[serde(default)]
    pub(super) hosts: Vec<crate::Host>,

    /// The client's identity configuration.
    pub(super) identity: crate::IdentityConfiguration,

    /// The client's authorization configuration.
    ///
    /// Defaulted, because every document already in a repository predates it.
    /// A required field here would make this model unable to read the very
    /// documents it wrote last week.
    #[serde(default)]
    pub(super) authorization: crate::AuthorizationConfiguration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spelled_out_pair_matches_the_two_constants() {
        assert_eq!(EXPECTED_DOCUMENT, format!("{API_VERSION}/{KIND}"));
    }
}
