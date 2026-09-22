//! Everything the control plane can refuse to do.

mod from_repository;

#[cfg(test)]
mod error_tests;
mod realm_unavailable_reason;
mod response;
mod status_mapping;

use fabric_client_model::{ClientId, DesiredStateError, RealmName};
use fabric_core::LogicalDataSourceName;

pub use realm_unavailable_reason::RealmUnavailableReason;

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
    /// No operator identity could be established: no bearer was presented.
    #[error(transparent)]
    Unauthenticated(#[from] OperatorAuthError),

    /// A bearer was presented, and the platform refused it.
    ///
    /// [`Self::Unauthenticated`]'s sibling, not a synonym: the two mean
    /// different things to whoever holds the bearer. No bearer at all asks
    /// "how do I sign in"; a refused one asks "why was I signed out", and
    /// ADR 0024's gateway probe (`apps/control-plane-ui/src/session/gateway.ts`)
    /// depends on being able to tell the two apart on the very first page
    /// load, before it has seen anything else. Carries nothing from the
    /// token or from why verification failed — no issuer, no `azp`, no role
    /// name — because this reaches a browser and the structured log already
    /// says why (`logging::operator_refused`); echoing any of it back would
    /// hand an attacker probing this route a free oracle for which part of a
    /// forged token was wrong.
    #[error("A bearer was presented and refused.")]
    OperatorRefused,

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

    /// The stored catalogue could not be read.
    ///
    /// [`Self::InvalidDesiredState`]'s sibling for the one document that is
    /// not a client's: same cause — a repository humans also edit by hand
    /// eventually holds a document that does not parse — and the same
    /// machine code, because a console reading either has one thing to do
    /// with it, stop and do not retry, not two different things to branch
    /// on. Kept as its own variant rather than a placeholder client id in
    /// [`Self::InvalidDesiredState`], because there is no client whose
    /// document broke.
    #[error("the stored catalogue could not be read: {source}")]
    InvalidCatalogue {
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

    /// The catalogue changed between being read and being written.
    ///
    /// [`Self::RevisionConflict`]'s sibling for the one document that is not
    /// a client's — an operator sees this from the Applications or Settings
    /// page, where "the *client* changed since it was read" names the wrong
    /// noun. The two share a machine code: both mean "read it again and redo
    /// the edit", and a console already knows which page it is showing, so
    /// it does not need a second code to know what to do.
    #[error("the catalogue changed since it was read; re-read it and apply the change again")]
    CatalogueRevisionConflict,

    /// A write would produce a document larger than this platform will
    /// store.
    ///
    /// Not a complaint about the request body — `PUT /identity` removing a
    /// single compromised redirect URI can still trip this, against a
    /// document that grew large from many earlier, unrelated writes. What
    /// is refused is the document the write would *produce*: GitHub's
    /// contents API cannot read a file back once it is over 1 MB, so a
    /// document that would cross that line is refused here, before the
    /// write, rather than discovered the next time somebody tries to read
    /// it back. See `document_size` for the two limits a write is checked
    /// against and why they differ. Answered as `422`, not `413`: `413`
    /// describes a request body that is itself too large, which this is
    /// not.
    #[error("the document this write would produce would exceed the {limit}-byte limit")]
    DocumentTooLarge {
        /// The limit that was exceeded.
        limit: usize,
    },

    /// A client with this id already exists.
    ///
    /// Its own variant beside [`Self::RevisionConflict`] rather than a reuse
    /// of it, because a create has no prior read to have gone stale: there
    /// is no revision this request believed it was editing, so "the client
    /// changed since it was read" would name a read that never happened.
    /// [`ClientService::create_client`](crate::ClientService::create_client)
    /// is the only place a repository [`Conflict`](crate::RepositoryError::Conflict)
    /// means this, so the translation happens there rather than in
    /// `ControlPlaneError::from_repository`, which every other write still
    /// uses.
    #[error("a client named {id} already exists")]
    ClientExists {
        /// The id that was already taken.
        id: ClientId,
    },

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

    /// A new client would take a realm that is reserved, or already used by
    /// another client.
    ///
    /// Says which. `ClientDocument::create` sets a new client's realm to its
    /// own id, and Keycloak's realm-create treats finding the realm already
    /// there as success — so a client id of `master`, or one matching a
    /// realm another client document already declares, would let the next
    /// reconciliation pass rewrite that realm using this operator's own
    /// authority. See
    /// [`ClientService::create_client`](crate::ClientService::create_client)
    /// for the whole argument. An earlier version of this message withheld
    /// which of the two reasons applied, reasoning that either answer would
    /// confirm some other client's realm exists — but every operator can
    /// already see every client's realm through `GET /api/clients`, so
    /// there was nothing left for that omission to protect, only a less
    /// useful message.
    #[error("the realm {realm} is unavailable: {reason}")]
    RealmUnavailable {
        /// The realm this id would have taken.
        realm: RealmName,
        /// Which of the two reasons this realm could not be taken.
        reason: RealmUnavailableReason,
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

    /// A declared data source breaks one of ADR 0023 part 1's rules.
    ///
    /// A `422` with the rule's own words as the message, not the `503` an
    /// unrecognised platform failure would otherwise get -- answered by
    /// the structural arm in `errors::status_mapping::platform`/`::codes`,
    /// which matches `Platform(PlatformError::InvalidDataSource(_))`
    /// directly, so this variant is never constructed in production and
    /// exists only for symmetry with the other named refusals here (and
    /// so a test can name it without reaching into `PlatformError`).
    #[error(transparent)]
    InvalidDataSource(#[from] fabric_platform_management::DataSourceRule),

    /// A client's document names no `spec.data` entry for the logical data
    /// source a placement was asked for (ADR 0023 part 2).
    ///
    /// [`fabric_platform_management::PlacementRefusal`]'s sibling for the
    /// one refusal that never reaches the selector: `select` chooses among
    /// what an environment declares and holds, and this request never gets
    /// that far because the client's own document names nothing to place.
    /// Sharing `PlacementRefused`'s `422` `placement_refused` is deliberate
    /// -- to an operator both mean "this cannot be placed", and the
    /// message says why.
    #[error("the client declares no data source named {logical}")]
    LogicalDataSourceNotDeclared {
        /// The logical data source the request named.
        logical: LogicalDataSourceName,
    },

    /// This deployment publishes no runtime state (ADR 0023 part 4).
    ///
    /// The route is mounted anyway, so a console can say what is missing --
    /// `PlatformNotManaged`'s own reasoning, one level in: a deployment can
    /// manage an environment's components and data sources while stating no
    /// `[platform_management.publication]` section, and an operator asking
    /// it to publish now needs to be told there is nowhere to publish to.
    /// Shares `PlatformNotManaged`'s `404` for the same reason, not a `503`:
    /// this is the same species of absence, fixed only by a config edit and
    /// a restart, and retrying in five seconds changes nothing.
    #[error("this deployment publishes no runtime state")]
    PublicationNotConfigured,

    /// Another publication pass -- scheduled or triggered -- was already in
    /// flight when this one was asked for.
    ///
    /// Not retried here: the guard releases the moment the in-flight pass
    /// finishes, which an operator who asked for "now" is better placed to
    /// judge than this API guessing a backoff for them.
    #[error("a publication pass is already running; try again shortly")]
    PublicationRunning,
}
