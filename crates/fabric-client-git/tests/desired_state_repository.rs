//! The Git-backed repository, against something that speaks the contents API.

// A test's helpers assert their own preconditions; `unwrap` there is the
// assertion, not a hole. Clippy's `allow-unwrap-in-tests` only covers
// `#[test]` functions, so an integration test file states it once here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::sync::Arc;
use std::time::Instant;

use fabric_client_git::{GitAuthConfig, GitClientRepository, GitCredential, GitRepositoryConfig};
use fabric_client_model::{ClientDocument, ClientId, ClientRevision, RoleName};
use fabric_control_plane::{ChangeContext, ClientRepository, RepositoryError};
use fabric_core::Clock;
use support::FakeGitHost;

/// A clock the token cache can measure against.
struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_700_000_000
    }
}

/// Where the fixture client's document lives.
const ACME_PATH: &str = "clients/acme/client.yaml";

/// A complete client document, with a section the model does not model.
const ACME: &str = r"apiVersion: fabric.fieldstate.nz/v1
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
    clients: []
  features:
    invoicing: true
";

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

fn change() -> ChangeContext {
    ChangeContext {
        requested_by: "brett@example.com".to_owned(),
        summary: "update identity for acme".to_owned(),
    }
}

/// Builds a repository pointed at a fake host.
fn repository(host: &FakeGitHost) -> GitClientRepository {
    let config = GitRepositoryConfig {
        api_base_url: host.base_url.clone(),
        owner: "FieldstateNZ".to_owned(),
        repository: "saas-fabric-clients".to_owned(),
        // The token posture, because these tests drive a socket rather than
        // GitHub and have no App to mint against. The App posture's own
        // behaviour is covered in `installation_tokens.rs`.
        auth: GitAuthConfig::Token {
            token_ref: "git/test".to_owned(),
        },
        ..GitRepositoryConfig::default()
    };

    GitClientRepository::new(
        &config,
        GitCredential::token("ghp_notarealtoken"),
        Arc::new(TestClock),
    )
    .expect("the repository must build")
}

/// The fixture document with one more role.
fn with_extra_role(document: &ClientDocument) -> ClientDocument {
    let mut identity = document.client().identity.clone();
    identity
        .roles
        .push(RoleName::try_new("Invoicing Approver").unwrap());

    document.with_identity(identity).unwrap()
}

#[tokio::test]
async fn a_client_is_read_with_its_revision() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    let stored = repository(&host).get(&client()).await.unwrap();

    assert_eq!(stored.document.client().display_name, "Acme");
    assert_eq!(stored.revision.as_str(), host.sha(ACME_PATH).unwrap());
}

#[tokio::test]
async fn a_write_at_the_current_revision_is_accepted_and_moves_it() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    let repository = repository(&host);
    let stored = repository.get(&client()).await.unwrap();

    let updated = with_extra_role(&stored.document);
    let revision = repository
        .update(&client(), &updated, &stored.revision, &change())
        .await
        .unwrap();

    assert_ne!(revision, stored.revision);
    assert_eq!(revision.as_str(), host.sha(ACME_PATH).unwrap());
    assert!(host.text(ACME_PATH).unwrap().contains("Invoicing Approver"));
}

#[tokio::test]
async fn a_write_at_a_stale_revision_is_refused_and_changes_nothing() {
    // The lost update this mechanism exists to prevent: somebody else
    // committed between the read and the write.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    let repository = repository(&host);
    let stored = repository.get(&client()).await.unwrap();

    host.overwrite(ACME_PATH, ACME);
    let after_other_write = host.text(ACME_PATH).unwrap();

    let error = repository
        .update(
            &client(),
            &with_extra_role(&stored.document),
            &stored.revision,
            &change(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, RepositoryError::Conflict));
    assert_eq!(host.text(ACME_PATH).unwrap(), after_other_write);
}

#[tokio::test]
async fn a_write_carries_the_revision_it_expects_rather_than_overwriting_blindly() {
    // The structural half of the same guarantee: if the hash were omitted, the
    // host would apply every write unconditionally and the test above would
    // pass only by luck.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    let repository = repository(&host);
    let stored = repository.get(&client()).await.unwrap();

    repository
        .update(
            &client(),
            &with_extra_role(&stored.document),
            &stored.revision,
            &change(),
        )
        .await
        .unwrap();

    let write = host.requests_with("PUT").into_iter().next().expect("a write");

    assert!(write.body.contains(&format!("\"sha\":\"{}\"", stored.revision)));
}

#[tokio::test]
async fn what_is_committed_parses_back_as_the_document_that_was_written() {
    // "Malformed desired state is not committed", asserted rather than argued:
    // the bytes that reached the host are read with the same parser that will
    // read them back.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    let repository = repository(&host);
    let stored = repository.get(&client()).await.unwrap();
    let updated = with_extra_role(&stored.document);

    repository
        .update(&client(), &updated, &stored.revision, &change())
        .await
        .unwrap();

    let committed = ClientDocument::parse(&host.text(ACME_PATH).unwrap())
        .expect("the committed bytes must parse as a client document");

    assert_eq!(committed.client(), updated.client());
}

#[tokio::test]
async fn an_edit_preserves_sections_the_model_does_not_understand() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    let repository = repository(&host);
    let stored = repository.get(&client()).await.unwrap();

    repository
        .update(
            &client(),
            &with_extra_role(&stored.document),
            &stored.revision,
            &change(),
        )
        .await
        .unwrap();

    assert!(host.text(ACME_PATH).unwrap().contains("invoicing: true"));
}

#[tokio::test]
async fn the_commit_message_names_the_operation_and_who_asked() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    let repository = repository(&host);
    let stored = repository.get(&client()).await.unwrap();

    repository
        .update(
            &client(),
            &with_extra_role(&stored.document),
            &stored.revision,
            &change(),
        )
        .await
        .unwrap();

    let write = host.requests_with("PUT").into_iter().next().expect("a write");

    assert!(write.body.contains("update identity for acme"));
    assert!(write.body.contains("Requested-by: brett@example.com"));
}

#[tokio::test]
async fn a_stored_document_that_will_not_parse_names_the_client() {
    let host = FakeGitHost::start(&[(ACME_PATH, "kind: Tenant\n")]).await;

    let error = repository(&host).get(&client()).await.unwrap_err();

    assert!(
        matches!(error, RepositoryError::Invalid { ref client, .. } if client.as_str() == "acme"),
        "{error}"
    );
}

#[tokio::test]
async fn an_absent_client_is_reported_as_absent() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    let error = repository(&host)
        .get(&ClientId::try_new("nobody").unwrap())
        .await
        .unwrap_err();

    assert!(matches!(error, RepositoryError::NotFound { .. }));
}

#[tokio::test]
async fn listing_returns_every_client_in_the_directory() {
    let other = ACME.replace("acme", "beta").replace("Acme", "Beta");
    let host = FakeGitHost::start(&[(ACME_PATH, ACME), ("clients/beta/client.yaml", other.as_str())]).await;

    let clients = repository(&host).list().await.unwrap();

    assert_eq!(clients.len(), 2);
    assert_eq!(clients[0].document.client().id.as_str(), "acme");
    assert_eq!(clients[1].document.client().id.as_str(), "beta");
}

#[tokio::test]
async fn a_directory_with_no_document_is_skipped_rather_than_failing_the_listing() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;
    host.add_empty_directory("clients/half-created/placeholder");

    let clients = repository(&host).list().await.unwrap();

    assert_eq!(clients.len(), 1);
}

#[tokio::test]
async fn one_unreadable_document_fails_the_listing_rather_than_vanishing_from_it() {
    // A client silently missing from the console is the worst presentation of
    // a broken document: everything looks fine, and the one client needing
    // attention is the one nobody can see.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME), ("clients/beta/client.yaml", "kind: Tenant\n")]).await;

    let error = repository(&host).list().await.unwrap_err();

    assert!(matches!(error, RepositoryError::Invalid { .. }));
}

#[tokio::test]
async fn every_request_presents_the_platform_credential() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    repository(&host).list().await.unwrap();

    assert!(
        host.requests()
            .iter()
            .all(|request| request.authorization.as_deref() == Some("Bearer ghp_notarealtoken")),
        "a request went out without the platform credential"
    );
}

#[tokio::test]
async fn a_failure_never_carries_the_credential_or_the_hosts_own_message() {
    let host = FakeGitHost::start(&[]).await;

    let error = repository(&host).list().await.unwrap_err().to_string();

    assert!(!error.contains("ghp_"));
    assert!(!error.contains("Not Found"));
}

#[tokio::test]
async fn the_description_names_the_repository_and_no_credential() {
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    let description = repository(&host).describe();

    assert!(description.contains("saas-fabric-clients"));
    assert!(description.contains("main"));
    assert!(!description.contains("ghp_"));
}

#[tokio::test]
async fn a_revision_the_host_reports_is_carried_opaquely() {
    // Nothing above the repository parses a revision, so a host that changed
    // its hash format would not break anything but this assertion.
    let host = FakeGitHost::start(&[(ACME_PATH, ACME)]).await;

    let stored = repository(&host).get(&client()).await.unwrap();

    assert_eq!(stored.revision, ClientRevision::try_new("sha-0").unwrap());
}
