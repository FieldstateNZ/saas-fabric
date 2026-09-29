//! Selecting a component version: `POST /api/catalogue`'s
//! `selectComponentVersion` (ADR 0026 section 7).

use fabric_client_model::catalogue::StoredCatalogue;
use fabric_client_model::{ClientId, ClientRevision};
use fabric_component::{ComponentVersion, Digest, Repository};

use crate::audit::{component_selected, Selecting, SelectionOutcome};
use crate::resolution::selectable;
use crate::state::ControlPlaneState;
use crate::{ControlPlaneError, Operator};

/// What an operator asked to select.
pub(super) struct Selection {
    /// The application.
    pub(super) application: ClientId,

    /// The component: a new one when no component has this id.
    pub(super) component: ClientId,

    /// The primary image's repository.
    pub(super) repository: Repository,

    /// The version tag.
    pub(super) version: ComponentVersion,
}

/// Selects a component version, and audits how it turned out.
///
/// # The order, and why the handler holds it
///
/// Three steps across two services, in the order ADR 0026 section 7 gives:
///
/// 1. `ClientService` reads the catalogue at the request's revision, and
///    the catalogue says whether the application exists and whether the
///    component is a capability — each refused before any registry read;
/// 2. the resolution service, which alone reads registries, refuses an
///    unregistered repository before any request and resolves the rest
///    under its deadline;
/// 3. `ClientService` writes the resolution through the catalogue's pure
///    `select_component` and saves it as every catalogue change is saved.
///
/// Neither service calls the other: `ClientService` stays a service that
/// calls no platform service, and the resolution writes nothing.
///
/// # Errors
///
/// Whatever step refused, each audited with its code.
pub(super) async fn select_component(
    state: &ControlPlaneState,
    operator: &Operator,
    selection: &Selection,
    expected: Option<&ClientRevision>,
) -> Result<StoredCatalogue, ControlPlaneError> {
    let outcome = attempt(state, operator, selection, expected).await;

    let entry = format!("{}/{}", selection.application, selection.component);
    let selecting = Selecting {
        entry: &entry,
        repository: selection.repository.as_str(),
        version: selection.version.as_str(),
    };
    match &outcome {
        Ok((_, primary_digest, descriptor_digest)) => {
            let selected = SelectionOutcome::Selected {
                primary_digest: primary_digest.as_str(),
                descriptor_digest: descriptor_digest.as_str(),
            };
            component_selected(operator, &selecting, &selected);
        }
        Err(error) => component_selected(operator, &selecting, &SelectionOutcome::Refused(error)),
    }
    outcome.map(|(stored, _, _)| stored)
}

/// The three steps, and the digests the selection recorded.
async fn attempt(
    state: &ControlPlaneState,
    operator: &Operator,
    selection: &Selection,
    expected: Option<&ClientRevision>,
) -> Result<(StoredCatalogue, Digest, Digest), ControlPlaneError> {
    let current = state.service.catalogue_at(expected).await?;
    selectable(
        &current.stored.catalogue,
        &selection.application,
        &selection.component,
    )?;

    let resolution = state
        .resolution
        .resolve(&selection.repository, &selection.version)
        .await?;
    let digests = (
        resolution.primary_digest.clone(),
        resolution.descriptor_digest.clone(),
    );

    let stored = state
        .service
        .record_selection(
            operator,
            current,
            &selection.application,
            &selection.component,
            resolution,
            expected,
        )
        .await?;
    Ok((stored, digests.0, digests.1))
}
