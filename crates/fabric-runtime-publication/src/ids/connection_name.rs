//! Names one of a connector's pre-configured connections.

identifier_newtype!(
    /// The name of a connection a connector already holds configuration for, as
    /// it appears in a published
    /// [`ConnectionSelectorDocument`](crate::ConnectionSelectorDocument).
    ///
    /// The canonical type is `fabric_connector::ConnectionName`. See
    /// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
    /// rather than depending on the crate that owns it.
    ConnectionName,
    "connection name"
);
