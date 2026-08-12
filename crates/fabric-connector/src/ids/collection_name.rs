//! Names a physical collection inside a backend.

identifier_newtype!(
    /// The name of a physical collection — a table, a view, a document
    /// collection — as the connector knows it.
    ///
    /// This is the far side of the abstraction from
    /// [`LogicalResourceName`](fabric_core::LogicalResourceName). An
    /// application asks for `customers`; the Data API's catalogue maps that to
    /// a `CollectionName`, which may be `customer_records` or anything else.
    /// Applications never see this type (§2, §26).
    CollectionName,
    "collection name"
);
