//! Identifies one configured execution backend.

identifier_newtype!(
    /// Identifies a connector instance the platform can execute against.
    ///
    /// A connector is a *backend*, not a tenant and not a database. Several
    /// tenants normally share one connector — that is the point, and it is what
    /// keeps connection counts bounded (§22). A tenant's runtime binding names
    /// the connector to use and, separately, which connection within it.
    ///
    /// Typical values: `postgres-au-east`, `sqlserver-primary`, `analytics`.
    ///
    /// # Examples
    ///
    /// ```
    /// use fabric_connector::ConnectorId;
    ///
    /// let connector = ConnectorId::try_new("postgres-au-east")?;
    /// assert_eq!(connector.as_str(), "postgres-au-east");
    /// # Ok::<(), fabric_core::IdentifierError>(())
    /// ```
    ConnectorId,
    "connector id"
);
