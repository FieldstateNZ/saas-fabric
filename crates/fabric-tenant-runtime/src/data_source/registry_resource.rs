//! How a DataSource plugs into the generic registry.

use fabric_core::{BindingRevision, DataSourceId};

use crate::resource::RegistryResource;
use crate::{ConfigurationError, DataSource};

/// Registry integration for [`DataSource`].
///
/// Kept apart from the type's own file because it answers a different
/// question. `data_source_resource.rs` says *what a DataSource is*; this says
/// *how it participates in the reconciled-resource lifecycle* — what it is
/// keyed by, and which field the revision guard compares.
///
/// The same pair exists for [`TenantRuntimeBinding`](crate::TenantRuntimeBinding),
/// and keeping both in the same shape makes the two resources legible side by
/// side.
impl RegistryResource for DataSource {
    type Key = DataSourceId;

    const KIND: &'static str = "data source";

    fn key(&self) -> &Self::Key {
        &self.id
    }

    fn revision(&self) -> BindingRevision {
        self.revision
    }

    /// Delegates to the inherent [`DataSource::validate`], which is where the
    /// rules themselves live. Spelled as a path call rather than
    /// `self.validate()` so it is obvious to a reader that this is delegation
    /// and not recursion.
    fn validate(&self) -> Result<(), ConfigurationError> {
        DataSource::validate(self)
    }
}
