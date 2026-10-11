//! Writing `spec.lifecycle` without altering unrelated desired state.

#[cfg(test)]
#[path = "document_lifecycle_tests.rs"]
mod tests;

use serde_norway::Value;

use crate::{ClientDocument, DesiredStateError, LifecycleRecord};

impl ClientDocument {
    /// The recorded lifecycle.
    #[must_use]
    pub const fn lifecycle(&self) -> &LifecycleRecord {
        &self.client().lifecycle
    }

    /// Produces a copy of this document carrying `record` as its lifecycle,
    /// leaving every other section untouched.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError`] if the document has no `spec` mapping,
    /// or if the result would not parse, which includes a record that breaks
    /// the lifecycle's own invariants.
    pub fn with_lifecycle(&self, record: &LifecycleRecord) -> Result<Self, DesiredStateError> {
        let mut raw = self.raw().clone();
        let spec = raw
            .get_mut("spec")
            .and_then(Value::as_mapping_mut)
            .ok_or(DesiredStateError::MissingField { field: "spec" })?;
        spec.insert(
            Value::String("lifecycle".into()),
            serde_norway::to_value(record).map_err(|error| DesiredStateError::Malformed {
                detail: error.to_string(),
            })?,
        );

        Self::parse(&super::render::render(&raw)?)
    }
}
