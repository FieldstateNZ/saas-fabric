//! Names a schema within a shared database.

identifier_newtype!(
    /// A schema qualifier, used when tenants share a database but not a schema.
    ///
    /// This is the value that makes schema-per-tenant isolation work (§18), and
    /// it is the single most security-sensitive name in the crate: it is
    /// interpolated into a qualified collection reference, where SQL does not
    /// permit a bound parameter. Getting it wrong does not throw an error — it
    /// silently reads another tenant's rows.
    ///
    /// It is validated here, and it is derived from the tenant's runtime
    /// binding rather than from anything the caller sent.
    SchemaName,
    "schema name"
);
