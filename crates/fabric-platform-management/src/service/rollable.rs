//! Whether a component is one rollback can promise anything about.

use std::collections::BTreeMap;

use crate::service::PlatformError;
use crate::{ArtifactSource, ComponentDesired};

/// The registries a component can be rolled back through, or a refusal.
///
/// The one place the artifact kind decides whether rollback is offered at all,
/// so both the listing and the write get the same answer for the same reason.
///
/// # Errors
///
/// [`PlatformError::RollbackUnsupported`] for a kind whose versions are not
/// immutable.
pub(super) fn rollable<'a>(
    component: &str,
    desired: &'a ComponentDesired,
) -> Result<&'a BTreeMap<String, String>, PlatformError> {
    match &desired.source {
        ArtifactSource::Oci { repositories } => Ok(repositories),
        other @ ArtifactSource::Helm { .. } => Err(PlatformError::RollbackUnsupported {
            component: component.to_owned(),
            artifact: other.describe(),
        }),
    }
}
