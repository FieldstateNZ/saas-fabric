//! The one error every refusal in this crate is.

/// Why a value, a component descriptor or its authored source was refused.
///
/// # Why two variants, and not one per rule
///
/// Everything this crate checks is either a value that breaks a rule or a
/// document of a version this build does not read, and those two are the
/// only distinction a caller acts on differently: ADR 0026 section 2 has a
/// reader answer *invalid, naming the version it found* for the second,
/// never *undescribed*, and a caller that wants to say so needs the version
/// without parsing a message. Every other refusal is carried as its message,
/// because the catalogue's own validators already speak in messages its
/// console shows verbatim, and those messages are part of its API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    /// A value, or a document, that breaks a rule of this contract.
    #[error("{detail}")]
    Invalid {
        /// What was wrong, in words an operator can act on.
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
