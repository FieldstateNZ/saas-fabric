//! Product persistence uses the same desired-state authority as clients.
use super::GitClientRepository;
use crate::logging;
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

        // The same round-trip guarantee `write.rs::update` applies to a
        // client document: the rendered text is parsed with exactly the
        // code that will read it back, and a failure aborts before the
        // write rather than leaving a catalogue in Git this platform can no
        // longer read.
        Catalogue::parse(&text).map_err(|_| RepositoryError::Rejected {
            detail: "Invalid catalogue".into(),
        })?;

        let revision = self
            .host
            .write_catalogue(&text, expected, &message(change))
            .await?;

        logging::catalogue_written(&revision);

        Ok(revision)
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

        // Same guarantee as `write.rs::update`: parsed with exactly the code
        // that will read it back, before the write, not after.
        ClientDocument::parse(&text).map_err(|source| RepositoryError::Invalid {
            client: document.client().id.clone(),
            source,
        })?;

        let revision = self
            .host
            .create_document(&document.client().id, &text, &message(change))
            .await?;

        logging::client_written(&document.client().id, &revision);

        Ok(revision)
    }
}
fn message(change: &ChangeContext) -> String {
    format!("{}\n\nRequested-by: {}\n", change.summary, change.requested_by)
}
