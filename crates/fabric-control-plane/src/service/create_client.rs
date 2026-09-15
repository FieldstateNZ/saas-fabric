//! Creating a client is a single desired-state write, refused when the id or
//! the realm it would take is not this operator's to take.

use fabric_client_model::{catalogue::CreateClientRequest, ClientDocument};

use crate::{
    audit, document_size, ChangeContext, ClientService, ControlPlaneError, Operator, RepositoryError,
    StoredClient,
};

impl ClientService {
    /// Creates a client, checking duplicates atomically in the repository.
    ///
    /// # Errors
    ///
    /// Refuses invalid assignments, a realm this operator may not
    /// take — [`ControlPlaneError::RealmUnavailable`] — an occupied
    /// identifier — [`ControlPlaneError::ClientExists`] — a document too
    /// large to store, and other repository failures.
    pub async fn create_client(
        &self,
        operator: &Operator,
        request: CreateClientRequest,
    ) -> Result<StoredClient, ControlPlaneError> {
        let repository = self.repository.current();

        self.check_realm_available(&request.id).await?;

        let catalogue = repository
            .catalogue()
            .await
            .map_err(ControlPlaneError::from_repository)?;
        let mut product = catalogue
            .catalogue
            .resolve(&request.configuration, &[])
            .map_err(ControlPlaneError::InvalidRequest)?;
        product
            .activity
            .push(self.product_event(operator, &request.id, "Client created"));
        let document = ClientDocument::create(&request.id, &request.configuration, &product)
            .map_err(ControlPlaneError::InvalidRequest)?;

        // Measured before the write, not left to the repository to
        // discover: GitHub's contents API cannot read a file this size
        // back, so a document that grows past it must be refused here
        // rather than committed and then unreadable.
        document_size::check(&document.render().map_err(ControlPlaneError::InvalidRequest)?)?;

        let change = ChangeContext {
            requested_by: operator.subject().into(),
            summary: format!("create client {}", request.id),
        };

        // Neither a `Conflict` nor a `Rejected` from `create` is the one
        // unambiguous event either looks like on its own.
        //
        // `Conflict` is Git's `409`: ordinarily a lost race on the branch
        // ref, but two creates of the same id racing each other can also
        // land as this. `Rejected` is Git's `422` on a create specifically
        // (see `fabric_client_git`'s `create_status_failure`, which is why
        // this is not the same `422` an *update*'s `Conflict` also covers):
        // GitHub answers it both when the file already exists — a create
        // carries no `sha`, so there is nothing to be stale against — and
        // for a genuine validation failure this platform's own request
        // caused. Re-reading tells every one of these apart: a document now
        // at this id means the id really is taken (`ClientExists`,
        // whichever status reported it); nothing there after a `Conflict`
        // means the write can be retried (`RevisionConflict`); nothing
        // there after a `Rejected` means the request really was invalid,
        // and stays `Rejected` — not retryable, because nothing about
        // asking again would change the answer.
        let revision = match repository.create(&document, &change).await {
            Ok(revision) => revision,
            Err(lost_race @ (RepositoryError::Conflict | RepositoryError::Rejected { .. })) => {
                return Err(match repository.get(&request.id).await {
                    Ok(_) => ControlPlaneError::ClientExists {
                        id: request.id.clone(),
                    },
                    Err(RepositoryError::NotFound { .. }) => match lost_race {
                        RepositoryError::Conflict => ControlPlaneError::RevisionConflict,
                        other => ControlPlaneError::from_repository(other),
                    },
                    Err(other) => ControlPlaneError::from_repository(other),
                });
            }
            Err(other) => return Err(ControlPlaneError::from_repository(other)),
        };

        self.reconciliation
            .mark_pending(&request.id, revision.clone(), self.clock.now_unix_seconds());

        audit::client_created(operator, &request.id, &revision);

        Ok(StoredClient { document, revision })
    }
}
