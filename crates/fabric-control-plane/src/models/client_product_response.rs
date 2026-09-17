//! What a client's product configuration looks like on the wire.

use fabric_client_model::catalogue::{
    ApplicationAssignment, ApplicationComponent, ClientProduct, NavigationItem,
};
use fabric_client_model::ClientId;

use crate::models::{ClientResponse, ReconciliationResponse};
use crate::{ControlPlaneError, StoredClient};

/// A client's product configuration, plus what each assignment resolves to.
///
/// # Why `resolved` is computed here rather than read from the document
///
/// The stored document names an application, a published version and a plan.
/// It does not name which components and navigation items that combination
/// grants — that depends on the plan's features, which is a rule
/// ([`ApplicationAssignment::components`], [`ApplicationAssignment::navigation`])
/// rather than a stored fact. Answering it once here means the console never
/// re-derives entitlement from the plan and feature graph itself, which is
/// exactly the kind of duplicated rule this crate's rustdoc warns against.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientProductResponse {
    /// The client's overview.
    pub(crate) client: ClientResponse,

    /// The stored product configuration.
    pub(crate) product: ClientProduct,

    /// What each assignment resolves to, in the same order as
    /// `product.applications`.
    pub(crate) resolved: Vec<ResolvedApplication>,

    /// Where reconciliation stands for this revision.
    pub(crate) reconciliation: ReconciliationResponse,
}

impl ClientProductResponse {
    /// Renders a stored client's product configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError::InvalidDesiredState`] if the stored
    /// product section will not parse. `stored` came from the repository, not
    /// from this request, so a document that will not parse is the platform's
    /// problem — the same failure `GET /api/clients` reports for a client
    /// document that will not parse at all, not
    /// [`ControlPlaneError::InvalidRequest`].
    pub(crate) fn from_stored(
        stored: &StoredClient,
        reconciliation: ReconciliationResponse,
    ) -> Result<Self, ControlPlaneError> {
        let client_id = stored.document.client().id.clone();
        let product = stored
            .document
            .product()
            .map_err(|source| ControlPlaneError::InvalidDesiredState {
                client: client_id,
                source,
            })?;
        let resolved = product
            .applications
            .iter()
            .map(ResolvedApplication::from)
            .collect();

        Ok(Self {
            client: ClientResponse::from_stored(stored),
            product,
            resolved,
            reconciliation,
        })
    }
}

/// What one assignment grants, filtered down from the published definition by
/// the selected plan.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedApplication {
    /// Which application this resolves.
    pub(crate) application_id: ClientId,

    /// Components the selected plan includes.
    pub(crate) components: Vec<ApplicationComponent>,

    /// Navigation items the selected plan grants.
    pub(crate) navigation: Vec<NavigationItem>,
}

impl From<&ApplicationAssignment> for ResolvedApplication {
    fn from(assignment: &ApplicationAssignment) -> Self {
        Self {
            application_id: assignment.application_id.clone(),
            components: assignment.components().into_iter().cloned().collect(),
            navigation: assignment.navigation().into_iter().cloned().collect(),
        }
    }
}
