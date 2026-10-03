//! Writing a resolved selection: the catalogue's pure half of ADR 0026
//! section 7, saved as every catalogue change is.

use fabric_client_model::catalogue::{ComponentResolution, ComponentSelectionError, StoredCatalogue};

use crate::service::catalogue::CatalogueRead;
use fabric_client_model::{ClientId, ClientRevision};

use crate::{ClientService, ControlPlaneError, Operator, SelectionRefusal};

impl ClientService {
    /// Records `resolution` for `component` of `application` over `read`,
    /// the catalogue the selection was checked against, and saves it at
    /// `expected`, to the repository it was read from, as
    /// [`change_catalogue`](Self::change_catalogue) saves: size check,
    /// conflict mapping, audit.
    ///
    /// Takes the resolution, never a registry: resolving happened outside
    /// this service, which still calls no platform service.
    ///
    /// # Errors
    ///
    /// [`SelectionRefusal::AlreadySelected`] when the component already
    /// records that component descriptor, so nothing is written;
    /// [`ControlPlaneError::InvalidRequest`] for a catalogue the selection
    /// would make invalid; and every failure saving can give, a revision
    /// that moved while the version was resolved included.
    pub(crate) async fn record_selection(
        &self,
        operator: &Operator,
        read: CatalogueRead,
        application: &ClientId,
        component: &ClientId,
        resolution: ComponentResolution,
        expected: Option<&ClientRevision>,
    ) -> Result<StoredCatalogue, ControlPlaneError> {
        let updated = read
            .stored
            .catalogue
            .select_component(
                application,
                component,
                resolution,
                operator.subject(),
                self.clock.now_unix_seconds(),
            )
            .map_err(|error| match error {
                ComponentSelectionError::AlreadySelected {
                    application,
                    component,
                    descriptor_digest,
                } => ControlPlaneError::from(SelectionRefusal::AlreadySelected {
                    application,
                    component,
                    descriptor_digest: descriptor_digest.to_string(),
                }),
                ComponentSelectionError::Refused(refused) => ControlPlaneError::InvalidRequest(refused),
            })?;
        read.save(operator, "select_component_version", updated, expected)
            .await
    }
}
