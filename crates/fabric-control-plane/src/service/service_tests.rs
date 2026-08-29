//! Tests for the control plane's domain rules.

use std::sync::Arc;

use fabric_client_model::{ClientId, ClientRevision, IdentityConfiguration, RealmName, RoleName};
use fabric_reconciliation::{ReconciliationStatus, ReconciliationStatusStore};

use crate::fixtures::{repository_with_acme, FixedClock};
use crate::reconcile::ReconciliationTrigger;
use crate::repository::InMemoryClientRepository;
use crate::{ClientService, ControlPlaneError, Operator};

fn service(repository: Arc<InMemoryClientRepository>) -> ClientService {
    ClientService::new(
        crate::DesiredStateBinding::to(repository),
        Arc::new(ReconciliationStatusStore::new()),
        Arc::new(ReconciliationTrigger::new()),
        Arc::new(FixedClock),
    )
}

fn operator() -> Operator {
    Operator::new("brett@example.com")
}

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

fn role(name: &str) -> RoleName {
    RoleName::try_new(name).unwrap()
}

/// The fixture's identity, with an extra role.
fn with_extra_role() -> IdentityConfiguration {
    IdentityConfiguration {
        realm: RealmName::try_new("acme").unwrap(),
        roles: vec![
            role("Client Realm Administrator"),
            role("Client Realm User"),
            role("Invoicing Approver"),
        ],
        clients: Vec::new(),
    }
}

#[tokio::test]
async fn a_write_at_the_current_revision_is_accepted_and_moves_it() {
    let (repository, revision) = repository_with_acme();
    let service = service(repository);

    let updated = service
        .set_identity(&operator(), &client(), with_extra_role(), &revision)
        .await
        .unwrap();

    assert_ne!(updated.revision, revision);
    assert_eq!(updated.document.client().identity.roles.len(), 3);
}

#[tokio::test]
async fn a_write_at_a_stale_revision_is_refused() {
    let (repository, revision) = repository_with_acme();
    let service = service(Arc::clone(&repository));

    // Somebody else wrote first.
    service
        .set_identity(&operator(), &client(), with_extra_role(), &revision)
        .await
        .unwrap();

    let mut second = with_extra_role();
    second.roles.push(role("Analytics Viewer"));

    let error = service
        .set_identity(&operator(), &client(), second, &revision)
        .await
        .unwrap_err();

    assert!(matches!(error, ControlPlaneError::RevisionConflict));
}

#[tokio::test]
async fn a_refused_write_leaves_desired_state_exactly_as_it_was() {
    let (repository, revision) = repository_with_acme();
    let service = service(Arc::clone(&repository));
    let before = service.get(&client()).await.unwrap();

    let stale = ClientRevision::try_new("rev-999").unwrap();
    service
        .set_identity(&operator(), &client(), with_extra_role(), &stale)
        .await
        .unwrap_err();

    assert_eq!(service.get(&client()).await.unwrap(), before);
    assert_eq!(before.revision, revision);
}

#[tokio::test]
async fn changing_the_realm_is_refused() {
    // Reconciliation only adds, so a rename would create an empty realm and
    // abandon the one holding every user and session.
    let (repository, revision) = repository_with_acme();
    let service = service(repository);

    let moved = IdentityConfiguration {
        realm: RealmName::try_new("acme-two").unwrap(),
        ..with_extra_role()
    };

    let error = service
        .set_identity(&operator(), &client(), moved, &revision)
        .await
        .unwrap_err();

    assert!(matches!(error, ControlPlaneError::RealmImmutable { .. }));
}

#[tokio::test]
async fn removing_a_required_role_is_refused_and_nothing_is_written() {
    let (repository, revision) = repository_with_acme();
    let service = service(Arc::clone(&repository));

    let stripped = IdentityConfiguration {
        roles: vec![role("Client Realm User")],
        ..with_extra_role()
    };

    let error = service
        .set_identity(&operator(), &client(), stripped, &revision)
        .await
        .unwrap_err();

    assert!(matches!(error, ControlPlaneError::InvalidRequest(_)));
    assert_eq!(service.get(&client()).await.unwrap().revision, revision);
}

#[tokio::test]
async fn a_write_marks_reconciliation_pending_rather_than_applied() {
    // The property ADR 0008 exists for: writing desired state says nothing
    // about whether the identity provider has been changed.
    let (repository, revision) = repository_with_acme();
    let service = service(repository);

    let updated = service
        .set_identity(&operator(), &client(), with_extra_role(), &revision)
        .await
        .unwrap();

    assert_eq!(
        service.reconciliation(&updated).status,
        ReconciliationStatus::Pending
    );
}

#[tokio::test]
async fn writing_an_unchanged_identity_does_not_move_the_revision() {
    // A no-op commit would reset a converged client to `pending` and put an
    // empty change in the audit trail.
    let (repository, revision) = repository_with_acme();
    let service = service(repository);
    let unchanged = service
        .get(&client())
        .await
        .unwrap()
        .document
        .client()
        .identity
        .clone();

    let result = service
        .set_identity(&operator(), &client(), unchanged, &revision)
        .await
        .unwrap();

    assert_eq!(result.revision, revision);
}

#[tokio::test]
async fn a_stale_revision_is_refused_even_when_the_change_would_be_a_no_op() {
    // Otherwise the precondition means "unless it does not matter", which is
    // not something a caller could reason about.
    let (repository, revision) = repository_with_acme();
    let service = service(Arc::clone(&repository));
    let unchanged = service
        .get(&client())
        .await
        .unwrap()
        .document
        .client()
        .identity
        .clone();

    service
        .set_identity(&operator(), &client(), with_extra_role(), &revision)
        .await
        .unwrap();

    let error = service
        .set_identity(&operator(), &client(), unchanged, &revision)
        .await
        .unwrap_err();

    assert!(matches!(error, ControlPlaneError::RevisionConflict));
}

#[tokio::test]
async fn an_edit_preserves_sections_the_control_plane_does_not_model() {
    let (repository, revision) = repository_with_acme();
    let service = service(repository);

    let updated = service
        .set_identity(&operator(), &client(), with_extra_role(), &revision)
        .await
        .unwrap();

    assert!(updated.document.render().unwrap().contains("invoicing: true"));
}

#[tokio::test]
async fn an_unreadable_repository_is_reported_as_unavailable() {
    let (repository, _) = repository_with_acme();
    repository.set_unavailable(Some("connection refused".to_owned()));

    let error = service(repository).list().await.unwrap_err();

    assert!(matches!(error, ControlPlaneError::RepositoryUnavailable));
}

#[tokio::test]
async fn an_unknown_client_is_not_confused_with_an_empty_repository() {
    let (repository, _) = repository_with_acme();
    let service = service(repository);

    let unknown = ClientId::try_new("nobody").unwrap();

    assert!(matches!(
        service.get(&unknown).await.unwrap_err(),
        ControlPlaneError::UnknownClient(_)
    ));
    assert_eq!(service.list().await.unwrap().len(), 1);
}
