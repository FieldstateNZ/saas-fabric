//! Why a desired-state repository could not do what was asked.

use fabric_client_model::{ClientId, DesiredStateError};

/// A failure reading or writing desired state.
///
/// # Seven, not one
///
/// Because the control plane answers each differently, and an operator needs
/// to be able to tell them apart. A conflict means "read it again and redo
/// your edit". An unavailable repository means "this will probably work in a
/// minute". A refused credential means "the platform is misconfigured and no
/// amount of retrying will help". Collapsing them into one error would make
/// every failure look like the last one, which is the outcome specification
/// §23 exists to prevent.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    /// No desired-state repository has been established yet.
    ///
    /// Not a failure of the repository — there is no repository. The platform
    /// is running and healthy, and an operator has not yet connected it to
    /// where client desired state lives.
    ///
    /// It is its own variant rather than an [`Unavailable`](Self::Unavailable)
    /// with a friendly message, because the two lead somewhere completely
    /// different: unavailable means wait, and this means *do something*. The
    /// console renders a connect screen for one and an error for the other,
    /// and it can only tell them apart if the platform says which it is.
    #[error("no desired-state repository is configured")]
    NotConfigured,

    /// No such client.
    #[error("no client named {client}")]
    NotFound {
        /// The client that was asked for.
        client: ClientId,
    },

    /// The stored revision has moved on since it was read.
    ///
    /// The write was refused **entirely**. Nothing was merged, and nothing was
    /// applied on top of the newer state — a partial write here would be
    /// worse than the lost update it was trying to avoid.
    #[error("the client changed since it was read")]
    Conflict,

    /// The repository could not be reached, or failed internally.
    #[error("the desired-state repository is unavailable: {detail}")]
    Unavailable {
        /// What the adapter observed, with no upstream body and no credential
        /// in it.
        detail: String,
    },

    /// The platform's own credential for the repository was refused.
    #[error("the desired-state repository refused the platform's credential")]
    NotPermitted,

    /// The repository understood the request and refused it.
    ///
    /// Distinct from [`Self::Unavailable`] because no retry fixes it: the
    /// platform asked for something the repository will not do, which is a
    /// misconfiguration rather than a bad minute. Reporting it as unavailable
    /// would produce a retry loop over a problem that needs a human.
    #[error("the desired-state repository refused the request: {detail}")]
    Rejected {
        /// What the adapter observed, with no upstream body and no credential
        /// in it.
        detail: String,
    },

    /// The repository holds something this model cannot read.
    ///
    /// Not a caller error: whatever wrote the document may not have been this
    /// code, and a repository humans also edit by hand will eventually contain
    /// a document that does not parse. It is reported as a platform failure
    /// naming the client, rather than as "no such client" — an operator
    /// looking at a broken document needs to know it is broken, not to be told
    /// it is absent.
    #[error("the stored desired state for {client} could not be read: {source}")]
    Invalid {
        /// The client whose document could not be read.
        client: ClientId,

        /// What was wrong with it.
        #[source]
        source: DesiredStateError,
    },
}
