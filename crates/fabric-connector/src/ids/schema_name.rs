//! Names a schema within a shared database.

identifier_newtype!(
    /// A schema qualifier, for when tenants share a database but not a schema.
    ///
    /// This is the value schema-per-tenant isolation (§18) will rest on. Note
    /// the tense: nothing in this workspace interpolates it today. The model is
    /// a deferred capability (ADR 0006), and
    /// [`IsolationModel::schema`](crate::IsolationModel::schema) — the only way
    /// to read one — has no production caller.
    ///
    /// It is validated here anyway, and derived from the tenant's runtime
    /// binding rather than from anything a caller sent, because the eventual
    /// consumer will have no bound parameter available: a schema qualifier goes
    /// into a statement by interpolation, and a wrong one raises no error, it
    /// silently reads another tenant's rows. Validating at construction is the
    /// only point where that is cheap.
    SchemaName,
    "schema name"
);
