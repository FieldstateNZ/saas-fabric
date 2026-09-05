//! Names a field within a collection.

identifier_newtype!(
    /// The name of a field on a collection, as it appears in a discriminator
    /// column, a catalogue's `key_field`, or a catalogue's `queryable_fields`.
    ///
    /// The canonical type is `fabric_connector::FieldName`. See
    /// [`ConnectorId`](crate::ConnectorId) for why this crate re-declares it
    /// rather than depending on the crate that owns it.
    FieldName,
    "field name"
);
