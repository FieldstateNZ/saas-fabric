//! The client identifier — what an operator addresses in the control plane.

slug_newtype!(
    /// A validated client identifier, such as `acme`.
    ///
    /// The same string as the [`TenantId`](fabric_core::TenantId) the runtime
    /// plane resolves for that organisation, and deliberately not the same
    /// type — see this crate's documentation for why. It takes the strict DNS
    /// rule because it becomes a Keycloak realm name, a directory name in the
    /// desired-state repository, and a path segment in the control-plane API.
    ClientId,
    "client id",
    fabric_core::naming::parse_dns_label
);
