//! What can go wrong writing platform desired state.

/// A failure reading or writing the platform repository.
///
/// # `Conflict` and `Contended` are not the same answer
///
/// Both mean "somebody else was writing too", and they lead opposite ways.
/// [`Conflict`](Self::Conflict) means a file this write was editing changed
/// underneath it: the change cannot be applied without deciding what to do
/// about the other one, which is a person's decision. [`Contended`](Self::Contended)
/// means the branch kept moving for reasons that had nothing to do with these
/// files, and the write simply ran out of attempts — nothing needs deciding,
/// and trying again later is the whole remedy.
///
/// Collapsing them would either ask an operator to resolve a busy branch, or
/// silently retry past a real disagreement.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlatformGitError {
    /// A file this write was editing changed since it was read.
    #[error("{path} changed since it was read")]
    Conflict {
        /// The first path found to have moved.
        path: String,
    },

    /// The branch kept moving, and the write ran out of attempts.
    #[error("the platform repository is busy; the write was not applied")]
    Contended,

    /// No such file, or no such branch.
    #[error("{what} was not found in the platform repository")]
    NotFound {
        /// What was asked for. A path or a ref, never a credential.
        what: String,
    },

    /// The platform's credential was refused.
    #[error("the platform repository refused the platform's credential")]
    NotPermitted,

    /// The repository could not be reached, or failed internally.
    #[error("the platform repository is unavailable: {detail}")]
    Unavailable {
        /// What was observed, with no upstream body and no credential in it.
        detail: String,
    },

    /// The host understood the request and refused it.
    ///
    /// Distinct from [`Unavailable`](Self::Unavailable) because no retry fixes
    /// it: the platform asked for something the host will not do.
    #[error("the platform repository refused the request: {detail}")]
    Rejected {
        /// What was observed, with no upstream body and no credential in it.
        detail: String,
    },
}

impl From<fabric_git_host::TokenError> for PlatformGitError {
    /// Maps a credential failure into this adapter's vocabulary.
    ///
    /// The three variants survive intact, because each leads somewhere
    /// different — look at the App, wait and retry, or fix the request. The
    /// clients adapter maps the same failure into its own words; that both do
    /// so separately is the point of the shared crate reporting its own.
    fn from(error: fabric_git_host::TokenError) -> Self {
        match error {
            fabric_git_host::TokenError::NotPermitted => Self::NotPermitted,
            fabric_git_host::TokenError::Unavailable { detail } => Self::Unavailable { detail },
            fabric_git_host::TokenError::Rejected { detail } => Self::Rejected { detail },
        }
    }
}
