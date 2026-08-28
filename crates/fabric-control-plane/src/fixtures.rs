//! Fixtures shared by this crate's unit tests.

use std::sync::Arc;
use std::time::Instant;

use fabric_client_model::ClientDocument;
use fabric_core::Clock;

use crate::repository::InMemoryClientRepository;

/// A complete client document, including a section the model does not model.
pub(crate) const ACME: &str = r"
apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: acme
spec:
  displayName: Acme
  hosts:
    - www.example.com
  identity:
    realm: acme
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients:
      - id: web
        type: oidc
        redirectUris:
          - https://www.example.com/callback
  features:
    invoicing: true
";

/// Parses [`ACME`].
pub(crate) fn acme_document() -> ClientDocument {
    ClientDocument::parse(ACME).unwrap()
}

/// A repository holding one client, and the revision it is at.
pub(crate) fn repository_with_acme() -> (Arc<InMemoryClientRepository>, fabric_client_model::ClientRevision) {
    let repository = Arc::new(InMemoryClientRepository::new());
    let revision = repository.insert(acme_document()).unwrap();

    (repository, revision)
}

/// A clock that never moves, so an assertion on a timestamp is stable.
pub(crate) struct FixedClock;

impl FixedClock {
    /// The wall-clock second every fixture is stamped with.
    pub(crate) const UNIX_SECONDS: u64 = 1_700_000_000;
}

impl Clock for FixedClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        Self::UNIX_SECONDS
    }
}
