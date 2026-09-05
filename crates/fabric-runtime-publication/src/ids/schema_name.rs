//! Names the schema a published tenant binding isolates into.

identifier_newtype!(
    /// A schema qualifier, as it appears in a published
    /// [`IsolationModelDocument::Schema`](crate::IsolationModelDocument::Schema).
    ///
    /// The canonical type is `fabric_connector::SchemaName`. See
    /// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
    /// rather than depending on the crate that owns it. ADR 0018, Decision part 1
    /// names this type explicitly alongside `ConnectorId`, `ConnectionName`, and
    /// `FieldName` as one the producer must validate itself, so that a value
    /// either side accepts is a value the other accepts too.
    SchemaName,
    "schema name"
);
