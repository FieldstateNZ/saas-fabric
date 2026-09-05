//! Names the physical collection a published catalogue entry points at.

identifier_newtype!(
    /// The name of a physical collection — a table, a view, a document
    /// collection — as it appears in a published
    /// [`ResourceDefinitionDocument`](crate::ResourceDefinitionDocument).
    ///
    /// The canonical type is `fabric_connector::CollectionName`. See
    /// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
    /// rather than depending on the crate that owns it. ADR 0018, Decision part 1
    /// names this type explicitly alongside `ConnectorId`, `ConnectionName`, and
    /// `FieldName` as one the producer must validate itself: a `collection` value
    /// the producer accepted unchecked could fail the consumer's own parse at
    /// startup, taking the whole file — and, for the catalogue, the process —
    /// down with it.
    CollectionName,
    "collection name"
);
