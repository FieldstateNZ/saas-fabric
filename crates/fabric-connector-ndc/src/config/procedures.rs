//! How a collection's writes map onto connector procedures.

/// The procedures backing one collection's writes.
///
/// # Why writes need explicit configuration
///
/// Core NDC 0.2 has no generic insert/update/delete. The only mutation
/// operation is invoking a **procedure** the connector declares, and connectors
/// choose their own procedure names and argument shapes — `ndc-postgres`
/// generates `insert_customers`, another connector might expose
/// `customers_create`.
///
/// So this mapping cannot be inferred, and the platform does not try. A
/// collection with no mapping simply cannot be written to, and the attempt is
/// refused. Guessing a procedure name would be unwise for an insert and
/// indefensible for a delete.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionProcedures {
    /// The procedure backing inserts.
    #[serde(default)]
    pub insert: Option<ProcedureBinding>,

    /// The procedure backing updates.
    #[serde(default)]
    pub update: Option<ProcedureBinding>,

    /// The procedure backing deletes.
    #[serde(default)]
    pub delete: Option<ProcedureBinding>,
}

impl CollectionProcedures {
    /// Whether any write is possible on this collection.
    #[must_use]
    pub const fn is_writable(&self) -> bool {
        self.insert.is_some() || self.update.is_some() || self.delete.is_some()
    }

    /// The mappings that must carry a predicate, paired with their verb.
    ///
    /// Used by configuration validation. Inserts are absent by design: there is
    /// no predicate on an insert.
    pub(super) fn predicate_bearing(&self) -> [(&'static str, Option<&ProcedureBinding>); 2] {
        [("update", self.update.as_ref()), ("delete", self.delete.as_ref())]
    }

    /// Every mapping this collection declares, paired with its verb.
    ///
    /// Deliberately wider than [`Self::predicate_bearing`], for checks that
    /// hold whatever the verb is. Naming one argument twice is incoherent on an
    /// insert as much as on an update, and a check scoped to the predicate-
    /// bearing verbs would wave the insert through.
    pub(super) fn all(&self) -> [(&'static str, Option<&ProcedureBinding>); 3] {
        [
            ("insert", self.insert.as_ref()),
            ("update", self.update.as_ref()),
            ("delete", self.delete.as_ref()),
        ]
    }
}

/// One procedure and the argument names it expects.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureBinding {
    /// The procedure's name in the connector's schema.
    pub procedure: String,

    /// The argument carrying the payload — rows for an insert, field changes
    /// for an update.
    ///
    /// Must differ from [`Self::filter_argument`], and startup validation
    /// rejects a mapping where it does not: both are placed in the same
    /// argument map, so a shared name means the predicate lands on top of the
    /// payload and the write silently changes nothing.
    #[serde(default)]
    pub payload_argument: Option<String>,

    /// The argument carrying the predicate, for updates and deletes.
    ///
    /// A mapping for an update or delete that omits this is rejected at
    /// startup: without somewhere to put the predicate, the tenant scoping
    /// added by
    /// [`MutationSpec::for_target`](fabric_connector::MutationSpec::for_target)
    /// would be silently dropped, and the write would reach every tenant's rows.
    #[serde(default)]
    pub filter_argument: Option<String>,
}

impl ProcedureBinding {
    /// The payload and predicate argument names, when the mapping declares
    /// both.
    ///
    /// `None` when either is absent — a mapping naming only one of them has
    /// nothing to collide with, so validation has nothing to compare.
    pub(super) fn argument_names(&self) -> Option<(&String, &String)> {
        self.payload_argument.as_ref().zip(self.filter_argument.as_ref())
    }
}
