//! Names a field within a collection.

identifier_newtype!(
    /// The name of a field on a collection.
    ///
    /// Field names arrive from callers — in projections, filters, and sort
    /// specifications — so they are validated for the same reason collection
    /// names are: they reach a position in a generated query where a
    /// parameterised value cannot go.
    FieldName,
    "field name"
);
