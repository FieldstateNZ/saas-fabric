//! The one error every refusal in this crate is.

/// Why a value, a component descriptor or its authored source was refused.
///
/// # Why three variants, and not one per rule
///
/// Everything this crate checks is a value that breaks a rule, except two
/// refusals a caller acts on differently, which ADR 0026 section 3 has a
/// reader name in its closed list of reasons: a document of a version this
/// build does not read (*invalid, naming the version it found*, never
/// *undescribed*), and images on more than one registry (*an image on
/// another registry*). A caller that wants to say either needs it without
/// parsing a message. Every other refusal is carried as its message, because
/// the catalogue's own validators already speak in messages its console
/// shows verbatim, and those messages are part of its API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    /// A value, or a document, that breaks a rule of this contract.
    #[error("{detail}")]
    Invalid {
        /// What was wrong, in words an operator can act on.
        detail: String,
    },

    /// A component names images on more than one registry: every image is on
    /// the primary image's registry, host and port alike.
    #[error("{detail}")]
    OtherRegistry {
        /// Which registries, in words an operator can act on.
        detail: String,
    },

    /// A component descriptor of a version this build does not read.
    #[error("a component descriptor of version {found} is not one this build reads; this build reads v1")]
    UnsupportedVersion {
        /// The version the document or its artifact type named.
        found: String,
    },
}

/// Builds [`ContractError::Invalid`] from its message.
pub(crate) fn invalid(detail: impl Into<String>) -> ContractError {
    ContractError::Invalid {
        detail: detail.into(),
    }
}
