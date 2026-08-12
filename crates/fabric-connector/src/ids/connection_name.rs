//! Names one of a connector's pre-configured connections.

identifier_newtype!(
    /// The name of a connection that the connector already holds
    /// configuration for.
    ///
    /// This is the preferred way to route a tenant to its physical database:
    /// the connector is configured with a set of named connections, and each
    /// request names one. The credential stays inside the connector's
    /// configuration and never travels in a request body — which is why this is
    /// preferred over [`ConnectionSelector::Secret`](crate::ConnectionSelector).
    ConnectionName,
    "connection name"
);
