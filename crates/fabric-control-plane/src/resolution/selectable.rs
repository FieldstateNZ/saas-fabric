//! What the catalogue must say before any registry is asked.

use fabric_client_model::catalogue::{Catalogue, ComponentKind};
use fabric_client_model::{ClientId, DesiredStateError};

use crate::resolution::SelectionRefusal;
use crate::ControlPlaneError;

/// Refuses a selection the catalogue already rules out: an application that
/// does not exist, or a component that is a platform capability.
///
/// Checked before resolving, so neither costs a registry read. A component
/// id that names nothing is not refused: selecting creates it.
///
/// # Errors
///
/// [`ControlPlaneError::InvalidRequest`] for an unknown application — the
/// words `select_component` would use — and
/// [`SelectionRefusal::CapabilityNotSelectable`] for a capability.
pub(crate) fn selectable(
    catalogue: &Catalogue,
    application: &ClientId,
    component: &ClientId,
) -> Result<(), ControlPlaneError> {
    let draft = &catalogue
        .applications
        .iter()
        .find(|candidate| &candidate.id == application)
        .ok_or_else(|| {
            ControlPlaneError::InvalidRequest(DesiredStateError::InvalidField {
                field: "catalogue",
                detail: "Application does not exist".to_owned(),
            })
        })?
        .draft;
    let kind = draft
        .components
        .iter()
        .find(|candidate| &candidate.id == component)
        .map(|found| found.kind);
    match kind {
        Some(ComponentKind::Capability) => Err(SelectionRefusal::CapabilityNotSelectable {
            application: application.clone(),
            component: component.clone(),
        }
        .into()),
        None | Some(ComponentKind::Container | ComponentKind::Helm | ComponentKind::Described) => Ok(()),
    }
}
