//! Product persistence uses the same desired-state authority as clients.
use super::GitClientRepository;
use fabric_client_model::catalogue::{Catalogue, StoredCatalogue};
use fabric_client_model::{ClientDocument, ClientRevision};
use fabric_control_plane::{ChangeContext, RepositoryError};
impl GitClientRepository {
    pub(super) async fn catalogue_read(&self) -> Result<StoredCatalogue, RepositoryError> {
        let Some(stored) = self.host.read_catalogue().await? else {
            return Ok(StoredCatalogue {
                catalogue: Catalogue::default(),
                revision: None,
            });
        };
        // Not `Unavailable`: a catalogue that will not parse is not fixed by
        // asking Git again, so it is reported as `InvalidCatalogue`, carrying
        // the parse failure rather than discarding it.
        let catalogue =
            Catalogue::parse(&stored.text).map_err(|source| RepositoryError::InvalidCatalogue { source })?;
        Ok(StoredCatalogue {
            catalogue,
            revision: Some(stored.revision),
        })
    }
    pub(super) async fn catalogue_write(
        &self,
        catalogue: &Catalogue,
        expected: Option<&ClientRevision>,
        change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        let text = catalogue.render().map_err(|_| RepositoryError::Rejected {
            detail: "Invalid catalogue".into(),
        })?;
        self.host.write_catalogue(&text, expected, &message(change)).await
    }
    pub(super) async fn create_client(
        &self,
        document: &ClientDocument,
        change: &ChangeContext,
    ) -> Result<ClientRevision, RepositoryError> {
        let text = document.render().map_err(|source| RepositoryError::Invalid {
            client: document.client().id.clone(),
            source,
        })?;
        self.host
            .create_document(&document.client().id, &text, &message(change))
            .await
    }
}
fn message(change: &ChangeContext) -> String {
    format!("{}\n\nRequested-by: {}\n", change.summary, change.requested_by)
}
