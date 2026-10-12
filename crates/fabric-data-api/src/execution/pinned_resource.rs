//! The view a request pins, and the catalogue entry it reads from it.

use fabric_core::LogicalResourceName;
use fabric_tenant_runtime::{ResolutionView, ResolveError};

use crate::{DataApiError, DataApiService, ResourceCatalog, ResourceDefinition};

impl DataApiService {
    /// Pins the view a request reads, and the catalogue entry `name` names
    /// in it. The request keeps the view across its authorization await and
    /// resolves its tenant binding, DataSource and execution target on it
    /// too (G8), so a generation installed while it is authorising reaches
    /// the next request, never the rest of this one.
    ///
    /// The entry is `None` when no view is installed: there is no catalogue
    /// at all, and no other to fall back on. The caller authorises first, as
    /// a request to an unprimed runtime always was, and then answers
    /// [`Self::unavailable`].
    ///
    /// # Errors
    ///
    /// [`DataApiError::UnknownResource`] when the installed catalogue has
    /// no such resource: a fact about the API surface, answered before
    /// authorization (see `prepare`'s ordering).
    pub(super) fn pinned(
        &self,
        name: &LogicalResourceName,
    ) -> Result<(ResolutionView, Option<ResourceDefinition>), DataApiError> {
        let view = self.runtime.pin();
        let entry = view
            .catalogue()
            .and_then(|catalogue| catalogue.get::<ResourceCatalog>())
            .map(|catalogue| catalogue.resolve(name).cloned())
            .transpose()?;
        Ok((view, entry))
    }

    /// What a request with no catalogue installed is answered, once
    /// authorised.
    pub(super) const fn unavailable() -> DataApiError {
        DataApiError::Resolve(ResolveError::RuntimeUnavailable)
    }
}
