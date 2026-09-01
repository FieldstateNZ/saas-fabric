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
            IntegrationKind::ClientConfiguration,
            Arc::clone(&host) as Arc<dyn GitAppProvisioning>,
            Arc::clone(&secrets),
            Arc::clone(&store) as Arc<dyn IntegrationStore>,
            Arc::new(ClientConfigurationTarget::new(
                Arc::new(FakeFactory),
                Arc::clone(&binding),
            )),
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
        .load(IntegrationKind::ClientConfiguration)
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

    let record = serde_json::to_string(
        &harness
            .store
            .load(IntegrationKind::ClientConfiguration)
            .await
            .expect("readable")
            .expect("recorded"),
    )
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
        harness
            .store
            .load(IntegrationKind::ClientConfiguration)
            .await
            .expect("readable")
            .is_none(),
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
        .load(IntegrationKind::ClientConfiguration)
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
        harness
            .store
            .load(IntegrationKind::ClientConfiguration)
            .await
            .expect("readable")
            .is_none(),
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
    assert!(harness
        .store
        .load(IntegrationKind::ClientConfiguration)
        .await
        .expect("readable")
        .is_none());
}

#[tokio::test]
async fn several_reachable_repositories_are_not_guessed_between() {
    let harness = connected(&[("FieldstateNZ", "clients"), ("FieldstateNZ", "something-else")]).await;

    let integration = harness
        .store
        .load(IntegrationKind::ClientConfiguration)
        .await
        .expect("readable")
        .expect("recorded");

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
            .load(IntegrationKind::ClientConfiguration)
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

    assert!(harness
        .store
        .load(IntegrationKind::ClientConfiguration)
        .await
        .expect("readable")
        .is_none());
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
        IntegrationKind::ClientConfiguration,
        Arc::clone(&harness.host) as Arc<dyn GitAppProvisioning>,
        Arc::clone(&harness.secrets),
        Arc::clone(&harness.store) as Arc<dyn IntegrationStore>,
        Arc::new(ClientConfigurationTarget::new(
            Arc::new(FakeFactory),
            DesiredStateBinding::unconfigured(),
        )),
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

// ---------------------------------------------------------------------------
// Two flows over one store.
//
// The mechanism is shared; the integrations are not. What these pin is that
// sharing the mechanism did not quietly share anything else — an operator who
// connects client configuration has not connected platform management, and one
// who forgets either still has the other.
// ---------------------------------------------------------------------------

/// A target that records what was done to it, and nothing else.
#[derive(Default)]
struct RecordingTarget {
    /// Repositories it was pointed at.
    bound: Mutex<Vec<String>>,

    /// How many times it was told to forget.
    unbound: Mutex<usize>,

    /// Why it was told a connected integration does not work.
    unusable: Mutex<Vec<String>>,
}

impl IntegrationTarget for RecordingTarget {
    fn bind(&self, integration: &GitIntegration, _private_key: &SecretValue) -> Result<(), String> {
        self.bound.lock().expect("the fake is not poisoned").push(
            integration
                .repository()
                .map_or_else(|| "nothing".to_owned(), SelectedRepository::describe),
        );

        Ok(())
    }

    fn unbind(&self) {
        *self.unbound.lock().expect("the fake is not poisoned") += 1;
    }

    fn unusable(&self, detail: &str) {
        self.unusable
            .lock()
            .expect("the fake is not poisoned")
            .push(detail.to_owned());
    }
}

impl RecordingTarget {
    fn bindings(&self) -> Vec<String> {
        self.bound.lock().expect("the fake is not poisoned").clone()
    }
}

/// A second flow over the store and secrets an existing harness already uses.
///
/// The same two stores, deliberately: this is the deployment's shape, and it
/// is the shape in which a keying mistake shows up as one integration reading
/// the other's record.
fn platform_flow(harness: &Harness, target: &Arc<RecordingTarget>) -> GitIntegrationService {
    GitIntegrationService::new(
        IntegrationKind::PlatformManagement,
        Arc::clone(&harness.host) as Arc<dyn GitAppProvisioning>,
        Arc::clone(&harness.secrets),
        Arc::clone(&harness.store) as Arc<dyn IntegrationStore>,
        Arc::clone(target) as Arc<dyn IntegrationTarget>,
        Arc::new(FixedClock),
    )
}

/// Runs a flow to a chosen repository, as an operator would.
async fn connect(service: &GitIntegrationService, owner: &str, name: &str) {
    let request = service
        .begin_connection(&operator(), owner)
        .expect("the connection must start");

    service
        .complete_creation("the-code", &state_from(&request.post_url))
        .await
        .expect("creation must complete");

    let install = service
        .begin_install(&operator())
        .await
        .expect("the install must start");

    service
        .complete_install("42", &state_from(&install))
        .await
        .expect("the install must complete");

    service
        .choose_repository(&operator(), owner, name)
        .await
        .expect("the repository must be accepted");
}

#[tokio::test]
async fn connecting_client_configuration_connects_nothing_else() {
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;
    let target = Arc::new(RecordingTarget::default());
    let platform = platform_flow(&harness, &target);

    assert!(
        platform
            .current()
            .await
            .expect("the store must be readable")
            .is_none(),
        "a client connection must not leave a platform record behind"
    );
    assert!(
        target.bindings().is_empty(),
        "nor bind anything platform management would then read through"
    );
}

#[tokio::test]
async fn each_flow_keeps_its_key_where_only_it_looks() {
    // The strongest form of "one application's authority is not the other's":
    // there is no name under which platform management could find the key
    // client configuration was given.
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;

    assert!(
        harness
            .secrets
            .get(&SecretName::new(
                IntegrationKind::PlatformManagement.private_key()
            ))
            .await
            .expect("the store must be readable")
            .is_none(),
        "connecting one application must not put a key where the other reads"
    );

    assert_ne!(
        IntegrationKind::ClientConfiguration.private_key(),
        IntegrationKind::PlatformManagement.private_key()
    );
}

#[tokio::test]
async fn a_correlation_token_from_one_flow_cannot_complete_the_other() {
    let harness = harness(&[("FieldstateNZ", "saas-fabric-platform")]);
    let target = Arc::new(RecordingTarget::default());
    let platform = platform_flow(&harness, &target);

    let request = harness
        .service
        .begin_connection(&operator(), "FieldstateNZ")
        .expect("the connection must start");

    let outcome = platform
        .complete_creation("the-code", &state_from(&request.post_url))
        .await;

    assert!(
        matches!(outcome, Err(IntegrationError::NotOurFlow)),
        "a token issued by one flow must not create the other's application"
    );
}

#[tokio::test]
async fn disconnecting_platform_management_leaves_client_configuration_alone() {
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;
    let target = Arc::new(RecordingTarget::default());
    let platform = platform_flow(&harness, &target);

    connect(&platform, "FieldstateNZ", "saas-fabric-clients").await;

    platform
        .disconnect(&operator())
        .await
        .expect("the disconnect must succeed");

    assert!(
        harness
            .service
            .current()
            .await
            .expect("the store must be readable")
            .is_some(),
        "forgetting one integration must leave the other's record"
    );
    assert!(
        harness
            .secrets
            .get(&SecretName::new(
                IntegrationKind::ClientConfiguration.private_key()
            ))
            .await
            .expect("the store must be readable")
            .is_some(),
        "nor may it delete the other's key"
    );
    assert!(
        harness.binding.is_configured(),
        "nor unbind what the other connected"
    );
}

#[tokio::test]
async fn the_platform_repository_must_be_one_that_installation_reaches() {
    // The candidate list comes from the installation this application has, and
    // the choice is checked against it rather than trusted. There is no path
    // by which an operator points platform management at a repository by
    // typing an owner and a name — including one the *client* application can
    // reach, which is a different installation entirely.
    let harness = harness(&[("FieldstateNZ", "saas-fabric-platform")]);
    let target = Arc::new(RecordingTarget::default());
    let platform = platform_flow(&harness, &target);

    connect(&platform, "FieldstateNZ", "saas-fabric-platform").await;

    let outcome = platform
        .choose_repository(&operator(), "SomebodyElse", "their-secrets")
        .await;

    assert!(
        matches!(outcome, Err(IntegrationError::Refused(_))),
        "a repository the installation does not reach must not be accepted"
    );
    assert_eq!(
        target.bindings().last().map(String::as_str),
        Some("FieldstateNZ/saas-fabric-platform"),
        "and the refusal must leave what was already bound where it was"
    );
}

#[tokio::test]
async fn a_stored_integration_whose_key_is_gone_is_failing_rather_than_absent() {
    // An operator connected this. Telling them "nothing is connected" would
    // send them to connect it again instead of to the reason it stopped.
    let harness = connected(&[("FieldstateNZ", "saas-fabric-clients")]).await;
    let target = Arc::new(RecordingTarget::default());

    let platform = GitIntegrationService::new(
        IntegrationKind::PlatformManagement,
        Arc::clone(&harness.host) as Arc<dyn GitAppProvisioning>,
        // A store with no key in it, standing in for one that has lost it.
        Arc::new(InMemorySecretStore::new()),
        Arc::clone(&harness.store) as Arc<dyn IntegrationStore>,
        Arc::clone(&target) as Arc<dyn IntegrationTarget>,
        Arc::new(FixedClock),
    );

    // A record, put there without the key that belongs with it.
    harness
        .store
        .save(
            IntegrationKind::PlatformManagement,
            &harness
                .service
                .current()
                .await
                .expect("readable")
                .expect("recorded"),
        )
        .await
        .expect("the store must accept the record");

    platform.restore().await;

    assert_eq!(
        target.unusable.lock().expect("the fake is not poisoned").len(),
        1,
        "a record that cannot be bound must be reported as failing"
    );
    assert!(target.bindings().is_empty());
}
