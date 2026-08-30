//! What a SaaS Fabric client is.

use crate::{AuthorizationConfiguration, ClientId, Host, IdentityConfiguration};

/// A client's desired state, as far as this increment models it.
///
/// # This is a view, not the whole document
///
/// A real client document carries more than these four fields — feature
/// enablement, data placement, a configuration profile (platform specification
/// §4). This type is the part the control plane currently understands, and it
/// is deliberately *not* the thing that gets written back:
/// [`ClientDocument`](crate::ClientDocument) writes, and it edits the parsed
/// document in place so the sections modelled here cannot displace the ones
/// that are not.
///
/// Reading that boundary the other way round is the mistake worth avoiding: a
/// `Client` is complete enough to render an operator screen and to reconcile
/// identity from, and nowhere near complete enough to serialise as a client's
/// desired state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    /// Which client this is.
    pub id: ClientId,

    /// The name an operator sees, such as `Acme`.
    ///
    /// Free text rather than a validated newtype: it is displayed and never
    /// interpolated into a path, a URL, or a query. It is still bounded and
    /// checked for control characters when the document is parsed — a display
    /// name containing a newline would break every log line it appeared in.
    pub display_name: String,

    /// The hostnames this client's applications are reached on.
    ///
    /// Modelled here because they identify the client to an operator, and
    /// reconciled by nothing yet: routing is a later increment (ADR 0008).
    pub hosts: Vec<Host>,

    /// The client's identity configuration.
    pub identity: IdentityConfiguration,

    /// The client's authorization configuration.
    ///
    /// Empty for every document written before this section existed, and that
    /// reads as "not managed here" rather than "nobody may do anything" — see
    /// [`AuthorizationConfiguration`]. Reconciled by nothing yet (ADR 0013).
    pub authorization: AuthorizationConfiguration,
}
