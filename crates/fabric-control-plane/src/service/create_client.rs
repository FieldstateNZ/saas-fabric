//! Creating a client is a single desired-state write, refused when the id or
//! the realm it would take is not this operator's to take.
//!
//! In the 121–150 line band. The reason is that this is one method,
//! `create_client`, together with the one private helper it calls and
//! nothing else — the realm check needed its own name and its own rustdoc
//! precisely because the reasoning behind it is the load-bearing part of
//! this file, and folding it back into `create_client`'s body to save lines
//! would bury that reasoning inside a longer function rather than remove it.

use fabric_client_model::{
    catalogue::CreateClientRequest, ClientDocument, ClientId, DesiredStateError, RealmName,
};

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

        // A `Conflict` from `create` is not the one unambiguous event it
        // looks like. On Git, `status_failure` maps both a stale-blob `409`
        // and an unrelated validation `422` to this same variant — so it
        // could mean "this id is already a document" or "somebody else's
        // commit landed on the branch at the same moment, and this write
        // never happened at all". Re-reading tells them apart: a document
        // now at this id means the id really is taken (`ClientExists`, not
        // retryable); nothing there means the write can be retried
        // (`RevisionConflict`).
        let revision = match repository.create(&document, &change).await {
            Ok(revision) => revision,
            Err(RepositoryError::Conflict) => {
                return Err(match repository.get(&request.id).await {
                    Ok(_) => ControlPlaneError::ClientExists {
                        id: request.id.clone(),
                    },
                    Err(RepositoryError::NotFound { .. }) => ControlPlaneError::RevisionConflict,
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

    /// Refuses a realm that is reserved, or that another stored client
    /// already declares.
    ///
    /// # Why this matters
    ///
    /// [`ClientDocument::create`] sets a new client's realm to its own id,
    /// and reconciliation treats *any* realm the document names as this
    /// client's — including one that already exists for another reason.
    /// Keycloak answers `409` to a realm-create call that finds the realm
    /// already there, and this platform's own admin client treats that as
    /// success (see `fabric_keycloak::admin`'s `create`), so the very next
    /// sweep would rename that realm, add roles to it and write application
    /// clients into it — using this operator's own bearer. A client id of
    /// `master`, or one matching a realm another client document already
    /// declares by hand, is exactly the takeover this refuses before a
    /// document is ever written.
    ///
    /// # Errors
    ///
    /// Returns [`ControlPlaneError::RealmUnavailable`] if the realm this id
    /// would produce is reserved, or is already declared by another client's
    /// stored document.
    async fn check_realm_available(&self, id: &ClientId) -> Result<(), ControlPlaneError> {
        let realm = RealmName::try_new(id.as_str()).map_err(|error| {
            ControlPlaneError::InvalidRequest(DesiredStateError::InvalidField {
                field: "id",
                detail: error.to_string(),
            })
        })?;

        if self.reserved_realms().contains(&realm) {
            return Err(ControlPlaneError::RealmUnavailable { realm });
        }

        let clients = self
            .repository
            .current()
            .list()
            .await
            .map_err(ControlPlaneError::from_repository)?;

        // Excludes a client already stored under *this same* id: realm and
        // id are the same string by construction, so re-requesting an id
        // that already exists would otherwise always collide with itself
        // here first — reported as a realm conflict with some other tenant,
        // when it is a plain duplicate id. That one is `create`'s own job,
        // below, which answers it as `ClientExists`.
        if clients.iter().any(|stored| {
            stored.document.client().identity.realm == realm && stored.document.client().id != *id
        }) {
            return Err(ControlPlaneError::RealmUnavailable { realm });
        }

        Ok(())
    }
}
