//! The connection flow, end to end, against fakes that behave like the host.
//!
//! What these pin is the ordering the flow depends on: nothing is recorded
//! before it is proven, and a failure part-way through leaves the platform in
//! a state an operator can retry from rather than one only a human with store
//! access can repair.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use super::*;
use crate::fixtures::FixedClock;
use crate::repository::{DesiredStateBinding, InMemoryClientRepository};
use crate::{ClientRepository, Operator};

/// What the fake host will do next.
#[derive(Default)]
struct Behaviour {
    /// Repositories the installation reaches.
    repositories: Vec<AccessibleRepository>,

    /// Whether redeeming a creation code succeeds.
    redemption_refused: bool,

    /// Whether a token can be minted for the installation.
    mint_refused: bool,
}

/// A Git host that does what a test tells it to.
#[derive(Default)]
struct FakeHost {
    /// What it will do.
    behaviour: Mutex<Behaviour>,

    /// How many times an installation was inspected.
    inspections: Mutex<usize>,
}

impl FakeHost {
    fn reaching(repositories: &[(&str, &str)]) -> Arc<Self> {
        Arc::new(Self {
            behaviour: Mutex::new(Behaviour {
                repositories: repositories
                    .iter()
                    .map(|(owner, name)| AccessibleRepository {
                        owner: (*owner).to_owned(),
                        name: (*name).to_owned(),
                        default_branch: "main".to_owned(),
                    })
                    .collect(),
                ..Behaviour::default()
            }),
            inspections: Mutex::new(0),
        })
    }

    fn refuse_minting(&self) {
        self.behaviour
            .lock()
            .expect("the fake is not poisoned")
            .mint_refused = true;
    }

    fn refuse_redemption(&self) {
        self.behaviour
            .lock()
            .expect("the fake is not poisoned")
            .redemption_refused = true;
    }
}

#[async_trait]
impl GitAppProvisioning for FakeHost {
    fn creation_request(&self, organisation: &str, state: &str) -> AppCreationRequest {
        AppCreationRequest {
            post_url: format!("https://host.test/organizations/{organisation}?state={state}"),
            manifest: serde_json::json!({ "name": "SaaS Fabric" }),
        }
    }

    async fn redeem_creation(&self, _code: &str) -> Result<CreatedApp, ProvisioningError> {
        if self
            .behaviour
            .lock()
            .expect("the fake is not poisoned")
            .redemption_refused
        {
            return Err(ProvisioningError::Refused);
        }

        Ok(CreatedApp {
            app_id: "1234".to_owned(),
            app_slug: "saas-fabric-acme".to_owned(),
            private_key: SecretValue::new("-----BEGIN RSA PRIVATE KEY-----fake"),
        })
    }

    fn install_url(&self, app_slug: &str, state: &str) -> String {
        format!("https://host.test/apps/{app_slug}/installations/new?state={state}")
    }

    async fn inspect_installation(
        &self,
        _app_id: &str,
        _private_key: &SecretValue,
        _installation_id: &str,
    ) -> Result<InstallationDetail, ProvisioningError> {
        *self.inspections.lock().expect("the fake is not poisoned") += 1;

        let behaviour = self.behaviour.lock().expect("the fake is not poisoned");

        if behaviour.mint_refused {
            return Err(ProvisioningError::Refused);
        }

        Ok(InstallationDetail {
            account: "FieldstateNZ".to_owned(),
            repositories: behaviour.repositories.clone(),
        })
    }
}

/// A factory that hands back an empty in-memory repository.
struct FakeFactory;

impl DesiredStateFactory for FakeFactory {
    fn build(
        &self,
        _integration: &GitIntegration,
        _private_key: &SecretValue,
    ) -> Result<Arc<dyn ClientRepository>, String> {
        Ok(Arc::new(InMemoryClientRepository::new()))
    }
}

/// A secret store that refuses every write.
#[derive(Default)]
struct RefusingSecrets;

#[async_trait]
impl SecretStore for RefusingSecrets {
    async fn get(&self, _name: &SecretName) -> Result<Option<SecretValue>, SecretStoreError> {
        Ok(None)
    }

    async fn put(&self, _name: &SecretName, _value: &SecretValue) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }

    async fn delete(&self, _name: &SecretName) -> Result<(), SecretStoreError> {
        Ok(())
    }

    fn describe(&self) -> String {
        "a store that refuses writes".to_owned()
    }
}

/// Everything a test drives.
struct Harness {
    service: GitIntegrationService,
    host: Arc<FakeHost>,
    store: Arc<InMemoryIntegrationStore>,
    secrets: Arc<dyn SecretStore>,
    binding: Arc<DesiredStateBinding>,
}

fn harness_with(host: Arc<FakeHost>, secrets: Arc<dyn SecretStore>) -> Harness {
    let store = Arc::new(InMemoryIntegrationStore::new());
    let binding = DesiredStateBinding::unconfigured();

    Harness {
        service: GitIntegrationService::new(
            Arc::clone(&host) as Arc<dyn GitAppProvisioning>,
            Arc::clone(&secrets),
            Arc::clone(&store) as Arc<dyn IntegrationStore>,
            Arc::new(FakeFactory),
            Arc::clone(&binding),
            Arc::new(FixedClock),
        ),
        host,
        store,
        secrets,
        binding,
    }
}

fn harness(repositories: &[(&str, &str)]) -> Harness {
    harness_with(
        FakeHost::reaching(repositories),
        Arc::new(InMemorySecretStore::new()),
    )
}

fn operator() -> Operator {
    Operator::new(
        "brett@example.com",
        crate::OperatorToken::new("an-operators-bearer"),
    )
}

/// Pulls the correlation token out of whatever the fake put it in.
fn state_from(url: &str) -> String {
    url.split("state=")
        .nth(1)
        .expect("the fake carries the state in the URL")
        .to_owned()
}

/// Runs the whole flow and returns the harness it ran against.
async fn connected(repositories: &[(&str, &str)]) -> Harness {
    let harness = harness(repositories);

    let request = harness
        .service
        .begin_connection(&operator(), "FieldstateNZ")
        .expect("the connection must start");

    harness
        .service
        .complete_creation("the-code", &state_from(&request.post_url))
        .await
        .expect("creation must complete");

    let install = harness
        .service
        .begin_install(&operator())
        .await
        .expect("the install must start");

    harness
        .service
        .complete_install("42", &state_from(&install))
        .await
        .expect("the install must complete");

    harness
}

#[tokio::test]
async fn a_completed_flow_records_the_application_and_binds_desired_state() {
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;

    let integration = harness
        .store
        .load()
        .await
        .expect("the store must be readable")
        .expect("an integration must have been recorded");

    assert_eq!(integration.app_id, "1234");
    assert_eq!(
        integration.repository().map(SelectedRepository::describe),
        Some("FieldstateNZ/saas-fabric-clients".to_owned())
    );
    assert!(
        harness.binding.is_configured(),
        "a completed connection must make desired state readable without a restart"
    );
}

#[tokio::test]
async fn the_private_key_is_stored_and_never_lands_in_the_record() {
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;

    let stored = harness
        .secrets
        .get(&SecretName::new("git/app-private-key"))
        .await
        .expect("the store must be readable");

    assert!(stored.is_some(), "the key arrives once and must be kept");

    let record = serde_json::to_string(&harness.store.load().await.expect("readable").expect("recorded"))
        .expect("the record must serialise");

    assert!(
        !record.contains("BEGIN RSA"),
        "the record is what the API may describe to an operator: {record}"
    );
}

#[tokio::test]
async fn a_callback_with_a_token_this_platform_never_issued_establishes_nothing() {
    let harness = harness(&[("FieldstateNZ", "saas-fabric-clients")]);

    let outcome = harness
        .service
        .complete_creation("the-code", "a-token-from-somewhere-else")
        .await;

    assert_eq!(outcome, Err(IntegrationError::NotOurFlow));
    assert!(
        harness.store.load().await.expect("readable").is_none(),
        "an anonymous callback must not be able to establish an integration"
    );
}

#[tokio::test]
async fn a_creation_callback_cannot_be_replayed() {
    let harness = harness(&[("FieldstateNZ", "saas-fabric-clients")]);

    let request = harness
        .service
        .begin_connection(&operator(), "FieldstateNZ")
        .expect("the connection must start");
    let state = state_from(&request.post_url);

    harness
        .service
        .complete_creation("the-code", &state)
        .await
        .expect("the first redemption must succeed");

    assert_eq!(
        harness.service.complete_creation("the-code", &state).await,
        Err(IntegrationError::NotOurFlow)
    );
}

#[tokio::test]
async fn an_installation_that_cannot_mint_a_token_is_not_recorded() {
    // The mint is the verification. Recording an installation the platform
    // cannot act as would give a console that says connected and a
    // reconciliation loop that fails every sweep.
    let harness = harness(&[("FieldstateNZ", "saas-fabric-clients")]);

    let request = harness
        .service
        .begin_connection(&operator(), "FieldstateNZ")
        .expect("start");
    harness
        .service
        .complete_creation("the-code", &state_from(&request.post_url))
        .await
        .expect("creation");

    harness.host.refuse_minting();

    let install = harness.service.begin_install(&operator()).await.expect("start");
    let outcome = harness
        .service
        .complete_install("42", &state_from(&install))
        .await;

    assert_eq!(outcome, Err(IntegrationError::HostRefused));

    let integration = harness
        .store
        .load()
        .await
        .expect("readable")
        .expect("the application is still recorded");

    assert!(
        integration.installation.is_none(),
        "an unverified installation must not be recorded"
    );
    assert!(!harness.binding.is_configured());
}

#[tokio::test]
async fn a_key_that_cannot_be_stored_leaves_no_half_made_integration() {
    // The key arrives exactly once. A record written without it would describe
    // an application this platform can never authenticate as, and no retry
    // could fix it because the host will not hand the key over again.
    let harness = harness_with(
        FakeHost::reaching(&[("FieldstateNZ", "saas-fabric-clients")]),
        Arc::new(RefusingSecrets),
    );

    let request = harness
        .service
        .begin_connection(&operator(), "FieldstateNZ")
        .expect("start");

    let outcome = harness
        .service
        .complete_creation("the-code", &state_from(&request.post_url))
        .await;

    assert_eq!(outcome, Err(IntegrationError::Unavailable));
    assert!(
        harness.store.load().await.expect("readable").is_none(),
        "nothing may be recorded when the key could not be kept"
    );
}

#[tokio::test]
async fn a_refused_redemption_records_nothing() {
    let harness = harness(&[("FieldstateNZ", "saas-fabric-clients")]);
    harness.host.refuse_redemption();

    let request = harness
        .service
        .begin_connection(&operator(), "FieldstateNZ")
        .expect("start");

    assert_eq!(
        harness
            .service
            .complete_creation("a-spent-code", &state_from(&request.post_url))
            .await,
        Err(IntegrationError::HostRefused)
    );
    assert!(harness.store.load().await.expect("readable").is_none());
}

#[tokio::test]
async fn several_reachable_repositories_are_not_guessed_between() {
    let harness = connected(&[("FieldstateNZ", "clients"), ("FieldstateNZ", "something-else")]).await;

    let integration = harness.store.load().await.expect("readable").expect("recorded");

    assert!(integration.installation.is_some(), "the install is recorded");
    assert!(
        integration.repository().is_none(),
        "picking the wrong repository would write client configuration somewhere nobody expects"
    );
    assert!(
        !harness.binding.is_configured(),
        "and the platform must report itself unconfigured until somebody says which"
    );
}

#[tokio::test]
async fn choosing_a_repository_settles_it_and_binds() {
    let harness = connected(&[("FieldstateNZ", "clients"), ("FieldstateNZ", "other")]).await;

    harness
        .service
        .choose_repository(&operator(), "FieldstateNZ", "other")
        .await
        .expect("the choice must be accepted");

    assert_eq!(
        harness
            .store
            .load()
            .await
            .expect("readable")
            .expect("recorded")
            .repository()
            .map(SelectedRepository::describe),
        Some("FieldstateNZ/other".to_owned())
    );
    assert!(harness.binding.is_configured());
}

#[tokio::test]
async fn a_repository_the_installation_cannot_reach_is_refused() {
    // An operator choosing from a stale list must not be able to point the
    // platform at something it cannot read.
    let harness = connected(&[("FieldstateNZ", "clients"), ("FieldstateNZ", "other")]).await;

    let outcome = harness
        .service
        .choose_repository(&operator(), "SomebodyElse", "private-things")
        .await;

    assert!(matches!(outcome, Err(IntegrationError::Refused(_))));
}

#[tokio::test]
async fn disconnecting_forgets_the_key_the_record_and_the_binding() {
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;

    harness
        .service
        .disconnect(&operator())
        .await
        .expect("disconnect must succeed");

    assert!(harness.store.load().await.expect("readable").is_none());
    assert!(harness
        .secrets
        .get(&SecretName::new("git/app-private-key"))
        .await
        .expect("readable")
        .is_none());
    assert!(!harness.binding.is_configured());
}

#[tokio::test]
async fn a_stored_integration_is_restored_at_startup() {
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;

    // A fresh service over the same stores is what a restart looks like.
    let restarted = GitIntegrationService::new(
        Arc::clone(&harness.host) as Arc<dyn GitAppProvisioning>,
        Arc::clone(&harness.secrets),
        Arc::clone(&harness.store) as Arc<dyn IntegrationStore>,
        Arc::new(FakeFactory),
        DesiredStateBinding::unconfigured(),
        Arc::new(FixedClock),
    );

    restarted.restore().await;

    assert!(
        restarted
            .current()
            .await
            .expect("readable")
            .is_some_and(|integration| integration.is_usable()),
        "an operator should not have to reconnect after every restart"
    );
}

#[tokio::test]
async fn an_organisation_name_that_could_steer_a_url_is_refused() {
    let harness = harness(&[]);

    for hostile in ["../evil", "a/b", "with space", "", "-leading", &"x".repeat(40)] {
        assert!(
            harness.service.begin_connection(&operator(), hostile).is_err(),
            "{hostile:?} must not reach the URL an operator's browser is handed"
        );
    }
}
