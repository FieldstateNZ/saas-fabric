//! Everything the control plane can refuse to do.

mod from_repository;

#[cfg(test)]
mod error_tests;
mod response;
mod status_mapping;

use fabric_client_model::{ClientId, DesiredStateError, RealmName};

use crate::operator::OperatorAuthError;

/// A refused control-plane request.
///
/// # Why these are not the Data API's errors
///
/// The Data API answers an *application*, and its first rule is that no
/// physical infrastructure is ever named. This API answers a *platform
/// operator*, whose entire job is to know what the platform is doing, so it
/// says considerably more: which client, which revision, which validation rule.
///
/// Two things it still does not say, and the reasons are different from the
/// runtime's:
///
/// 1. **Nothing an upstream system said verbatim.** A Keycloak admin error
///    body or a Git provider's JSON is replaced with a Fabric error (§23), so
///    a browser never renders another system's internals and a log is the only
///    place they exist.
/// 2. **Nothing about the repository's internals.** Not a path, not a branch,
///    not a file (§8). "The client changed since you read it" is the operator's
///    problem; which blob moved is not.
///
/// # Why there is no "reconciliation pending" error
///
/// Because it is not a failure. §23 asks that it be distinguishable, and it is
/// — as a *status* on a successful response, not as an error. Reporting a
/// perfectly good write as an error because a downstream convergence has not
/// happened yet would make the normal path look broken.
#[derive(Debug, thiserror::Error)]
pub enum ControlPlaneError {
    /// No operator identity could be established.
    #[error(transparent)]
    Unauthenticated(#[from] OperatorAuthError),

    /// A secret operation failed. Carried rather than flattened: a stale
    /// write, an outage and a client with no boundary differ to an operator.
    #[error(transparent)]
    Secrets(#[from] crate::SecretsError),

    /// No such client.
    #[error("no client named {0}")]
    UnknownClient(ClientId),

    /// The operator sent something this model will not write.
    #[error("{0}")]
    InvalidRequest(DesiredStateError),

    /// The repository holds a document this model cannot read.
    ///
    /// Distinct from [`Self::InvalidRequest`] because nothing the operator did
    /// caused it and no correction to their request will fix it. One is a 400
    /// and the other a 500, and telling them apart is the point of having both.
    #[error("the stored desired state for {client} could not be read: {source}")]
    InvalidDesiredState {
        /// The client whose document could not be read.
        client: ClientId,

        /// What was wrong with it.
        #[source]
        source: DesiredStateError,
    },

    /// The request did not say which revision it was editing.
    #[error("this request must state the revision it is editing")]
    RevisionRequired,

    /// The client changed between being read and being written.
    #[error("the client changed since it was read; re-read it and apply the change again")]
    RevisionConflict,

    /// The operator asked to move a client to a different realm.
    ///
    /// Refused rather than reconciled. Reconciliation only adds, so a realm
    /// rename would create a second, empty realm and abandon the first — with
    /// every user, session and application client still in it. There is no
    /// safe way to express that as an edit to a document, so it is not
    /// expressible at all.
    #[error("a client's realm cannot be changed once it exists (currently {current})")]
    RealmImmutable {
        /// The realm the client is in.
        current: RealmName,
    },

    /// The desired-state repository could not be reached.
    #[error("the desired-state repository is unavailable")]
    RepositoryUnavailable,

    /// The platform's own credential for the repository was refused.
    #[error("the platform's desired-state credential was refused")]
    RepositoryDenied,

    /// The desired-state repository refused the platform's request.
    #[error("the desired-state repository refused the platform's request")]
    RepositoryRejected,

    /// No desired-state repository has been established yet.
    ///
    /// The platform is healthy; it has not been connected to where client
    /// desired state lives. Kept distinct from every other failure because it
    /// is the one an operator can fix from the console.
    #[error("this platform is not connected to a client desired-state repository yet")]
    IntegrationNotConfigured,

    /// A Git-host callback did not name a connection this platform started.
    ///
    /// One error for four causes — never issued, already spent, expired, or
    /// belonging to the other leg of the flow. They are the same thing to an
    /// operator, and distinguishing them in a response would tell whoever is
    /// guessing which guess was closest.
    #[error("that connection did not start here; start it again")]
    InvalidFlow,

    /// This deployment states where desired state lives, so there is nothing
    /// to connect.
    ///
    /// A deployment that names a repository has opted out of the managed path.
    /// Offering it a connection flow would be offering to overwrite a decision
    /// somebody made in a file, from a browser.
    #[error("this deployment states its desired-state repository; it is not managed here")]
    IntegrationNotManaged,

    /// This deployment has no platform repository connected.
    ///
    /// The route is mounted anyway, so a console can say what is missing. A
    /// route that did not exist would leave it reporting a 404 as though the
    /// operator had asked for the wrong thing.
    #[error("this platform manages no environments")]
    PlatformNotManaged,

    /// Platform Management could not answer.
    #[error(transparent)]
    Platform(#[from] fabric_platform_management::PlatformError),

    /// This deployment converges no identity provider.
    #[error("this platform converges no identity provider")]
    ConvergenceUnavailable,

    /// The Git host refused something the connection flow asked of it.
    #[error("the Git host refused the request")]
    GitHostRefused,

    /// The operator asked for something the platform will not do.
    #[error("{0}")]
    IntegrationRefused(String),

    /// The integration moved between being read and being written.
    ///
    /// The integration's own `revision_conflict`: an operator's
    /// choice of repository, or an installation callback, was prepared against
    /// a record and a key that a disconnect or another operator's rebind has
    /// since replaced. Nothing was written. Kept apart from
    /// [`Self::IntegrationRefused`] because the request was not wrong, and from
    /// [`Self::RepositoryUnavailable`] because nothing was unreachable.
    #[error("the integration changed while this request was being prepared; look again and ask again")]
    IntegrationMoved,

    /// The identity provider refused to redeem an authorization code.
    #[error("the sign-in could not be completed; start again")]
    SignInRefused,

    /// The identity provider could not be reached to redeem a code.
    #[error("the identity provider is unavailable")]
    SignInUnavailable,
}
