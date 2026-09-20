//! What the host hands in to build the control plane.

use std::collections::BTreeSet;
use std::sync::Arc;

use fabric_core::Clock;

use crate::registration::{PlatformBinding, PublicationSink};
use crate::repository::DesiredStateBinding;

/// What the control plane is assembled from.
///
/// A struct rather than eight positional parameters. Half of them are
/// `Option<Arc<dyn …>>` and three of those are interchangeable at the call
/// site by type, which is the shape of argument list where a transposition
/// compiles and then behaves strangely at runtime.
pub struct ControlPlaneDeps {
    /// Where desired state is read and written, or the fact that it is not.
    pub desired_state: Arc<DesiredStateBinding>,

    /// Stamps writes and reconciliation outcomes.
    pub clock: Arc<dyn Clock>,

    /// The keys operator tokens are verified against.
    pub keys: Arc<crate::KeyHolder>,

    /// Lends each operator's authority to the identity provider.
    pub identity_provider: Option<Arc<dyn crate::IdentityProviderFactory>>,

    /// How an operator obtains a token.
    pub sign_in: Option<Arc<crate::SignInSurface>>,

    /// The Git connection flow, when this deployment manages its own.
    pub git_integration: Option<Arc<crate::GitIntegrationService>>,

    /// Where clients' secrets live, when a deployment has a store for them.
    ///
    /// `None` leaves the secrets routes mounted and answering "this client has
    /// no secret boundary" — the same answer a client without one gets, and
    /// for the same reason: the console can tell an operator what is missing
    /// rather than meeting a route that does not exist.
    pub client_secrets: Option<Arc<dyn crate::ClientSecrets>>,

    /// The flow that connects the platform repository, when this deployment
    /// manages an environment.
    ///
    /// Separate from `platform` because they become available at different
    /// times: the binding exists from startup and holds nothing, and this is
    /// what an operator uses to give it something to hold.
    pub platform_integration: Option<Arc<crate::GitIntegrationService>>,

    /// Platform Management, and the one environment it manages.
    ///
    /// One struct rather than two optional fields, because two that must agree
    /// is a shape that can disagree. A control plane manages the environment
    /// it was deployed into; there is no second one to name.
    ///
    /// `None` leaves the route mounted and answering that nothing is managed,
    /// so a console can say what is missing rather than meeting a 404 it would
    /// have to guess the meaning of.
    pub platform: Option<PlatformBinding>,

    /// Where this deployment publishes the runtime's three documents, when
    /// it publishes them at all (ADR 0023 part 4).
    ///
    /// `None` for a deployment that states no
    /// `[platform_management.publication]` section, whether or not it
    /// manages a platform — a deployment can manage an environment's
    /// components and data sources while stating nothing about publishing
    /// its runtime state, and the reverse is meaningless (there is nothing
    /// to publish without `platform` too). `build_control_plane` only ever
    /// constructs a publisher when both this and `platform` are `Some`.
    pub publication: Option<PublicationSink>,

    /// Establishes who an operator is, when something other than the
    /// configured posture should decide.
    ///
    /// `None` in every deployment: the posture in configuration is what builds
    /// it. It exists for tests, which drive the real router and would
    /// otherwise have to mint tokens signed by a key they also had to publish
    /// — proving the extractor works, and nothing else, at considerable cost.
    pub operators: Option<Arc<dyn crate::OperatorAuthenticator>>,

    /// Realms a new client may never declare, because something other than
    /// a client already means them: Keycloak's own `master`, the realm the
    /// operator posture itself authenticates against, and — when this
    /// deployment converges Keycloak — the admin realm its machine identity
    /// lives in.
    ///
    /// Computed here, at the composition root, rather than inside this
    /// crate: this crate knows no realm's name but a client's own, and
    /// deliberately — see [`ClientService`](crate::ClientService)'s own
    /// rustdoc for "not Keycloak". The names it is handed are opaque to it;
    /// only whoever assembles a deployment knows what they mean.
    ///
    /// Plain, case-folded strings, not [`RealmName`](fabric_client_model::RealmName)
    /// — see [`ClientService`](crate::ClientService)'s own field for why.
    pub reserved_realms: BTreeSet<String>,

    /// Application ids the catalogue may never accept, because something
    /// other than an application already means them: this platform's own
    /// OIDC client ids — the console's, and, when Keycloak is configured,
    /// its machine identity's. The realm-managed built-ins every realm
    /// carries (`account`, `realm-management`, …) are not here: they are
    /// static identity-protocol facts, not this deployment's configuration,
    /// so `fabric-client-model` refuses them on its own.
    ///
    /// Plain strings, not [`ClientId`](fabric_client_model::ClientId) — see
    /// [`ClientService`](crate::ClientService)'s own field for why.
    ///
    /// Computed the same way, and for the same reason, as `reserved_realms`.
    pub reserved_client_ids: BTreeSet<String>,
}
