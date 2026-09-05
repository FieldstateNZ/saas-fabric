//! Identifies the connector a published DataSource executes against.

identifier_newtype!(
    /// A connector identifier, as it appears in a published
    /// [`DataSourceDocument`](crate::DataSourceDocument).
    ///
    /// The canonical type is `fabric_connector::ConnectorId`, but that crate is
    /// runtime plane and this crate may depend on nothing but `fabric-core` (see
    /// `docs/architecture/crate-dependencies.md`). Re-declaring the newtype over
    /// the same [`fabric_core::naming::parse_identifier`] rule means the two
    /// copies cannot quietly drift apart: a value this crate accepts is a value
    /// the runtime accepts, because both ask the identical question.
    ///
    /// # Examples
    ///
    /// ```
    /// use fabric_runtime_publication::ConnectorId;
    ///
    /// let connector = ConnectorId::try_new("postgres-au-east")?;
    /// assert_eq!(connector.as_str(), "postgres-au-east");
    /// # Ok::<(), fabric_core::IdentifierError>(())
    /// ```
    ConnectorId,
    "connector id"
);
