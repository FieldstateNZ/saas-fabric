//! Adds operator history without altering unrelated desired state.
use crate::{catalogue::ProductActivity, ClientDocument, DesiredStateError};
use serde_norway::Value;
impl ClientDocument {
    /// Appends a server-attributed client activity event.
    /// # Errors
    /// Rejects malformed product state and serialization failures.
    pub fn with_activity(&self, event: ProductActivity) -> Result<Self, DesiredStateError> {
        let mut product = self.product()?;
        product.activity.push(event);
        let mut raw = self.raw().clone();
        let spec = raw
            .get_mut("spec")
            .and_then(Value::as_mapping_mut)
            .ok_or(DesiredStateError::MissingField { field: "spec" })?;
        spec.insert(
            Value::String("product".into()),
            serde_norway::to_value(product).map_err(|error| DesiredStateError::Malformed {
                detail: error.to_string(),
            })?,
        );
        Self::parse(&super::render::render(&raw)?)
    }
}
