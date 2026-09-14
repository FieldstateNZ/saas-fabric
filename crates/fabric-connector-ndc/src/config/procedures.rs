//! How a collection's writes map onto connector procedures.

use crate::config::ProcedureBinding;

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

    /// The mappings that must carry a payload, paired with their verb.
    ///
    /// Deletes are absent for the mirror-image reason inserts are absent from
    /// [`Self::predicate_bearing`]: there is nothing to send.
    pub(super) fn payload_bearing(&self) -> [(&'static str, Option<&ProcedureBinding>); 2] {
        [("insert", self.insert.as_ref()), ("update", self.update.as_ref())]
    }

    /// Every mapping this collection declares, paired with its verb.
    ///
    /// Deliberately wider than [`Self::predicate_bearing`], for checks that
    /// hold whatever the verb is. Naming one argument twice is incoherent on an
    /// insert as much as on an update, and a check scoped to the predicate-
    /// bearing verbs would wave the insert through. The startup check that
    /// every configured argument is one the procedure declares wants the same
    /// breadth, for the same reason.
    pub(crate) fn all(&self) -> [(&'static str, Option<&ProcedureBinding>); 3] {
        [
            ("insert", self.insert.as_ref()),
            ("update", self.update.as_ref()),
            ("delete", self.delete.as_ref()),
        ]
    }
}
