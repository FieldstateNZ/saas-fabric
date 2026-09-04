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

    /// Where `bind` and `unbind` wait, when a test wants to hold them there.
    ///
    /// Absent for every test that does not care, which is most of them: a
    /// target with no gate answers immediately, exactly as it always did.
    gate: Option<Arc<Gate>>,
}

#[async_trait::async_trait]
impl IntegrationTarget for RecordingTarget {
    async fn bind(&self, integration: &GitIntegration, _private_key: &SecretValue) -> Result<(), String> {
        self.arrive().await;

        self.bound.lock().expect("the fake is not poisoned").push(
            integration
                .repository()
                .map_or_else(|| "nothing".to_owned(), SelectedRepository::describe),
        );

        Ok(())
    }

    async fn unbind(&self) {
        self.arrive().await;

        *self.unbound.lock().expect("the fake is not poisoned") += 1;
    }

    async fn unusable(&self, detail: &str) {
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

    fn releases(&self) -> usize {
        *self.unbound.lock().expect("the fake is not poisoned")
    }

    /// A target whose every bind and unbind has to get past `gate` first.
    fn gated(gate: &Arc<Gate>) -> Arc<Self> {
        Arc::new(Self {
            gate: Some(Arc::clone(gate)),
            ..Self::default()
        })
    }

    /// Announces the call and waits for the test to let it through.
    async fn arrive(&self) {
        if let Some(gate) = self.gate.as_ref() {
            gate.pass().await;
        }
    }
}

/// A door a gated target waits at, and a bell it rings on the way in.
///
/// The bell is what makes these tests deterministic rather than timed: a test
/// can wait for the transition to be *inside* the target before it cancels the
/// request, instead of sleeping and hoping.
///
/// The door holds only the calls a test asked for, and that restraint is
/// load-bearing. A door that held every call would serialise transitions all by
/// itself, and a test over it could not tell the order this service keeps from
/// the order the fake kept on its behalf — which is exactly the property
/// `two_transitions_settle_on_the_last_one_asked_for` is about.
struct Gate {
    /// Rung once for every call that reaches the door.
    arrivals: tokio::sync::mpsc::UnboundedSender<()>,

    /// Held by the test for as long as it wants those calls parked.
    door: Arc<tokio::sync::Mutex<()>>,

    /// How many more arrivals wait at the door. Everything else walks past.
    holding: std::sync::atomic::AtomicUsize,
}

impl Gate {
    fn new() -> (Arc<Self>, tokio::sync::mpsc::UnboundedReceiver<()>) {
        let (arrivals, waiting) = tokio::sync::mpsc::unbounded_channel();

        let gate = Arc::new(Self {
            arrivals,
            door: Arc::new(tokio::sync::Mutex::new(())),
            holding: std::sync::atomic::AtomicUsize::new(0),
        });

        (gate, waiting)
    }

    /// Holds the next `count` calls at the door, for as long as it is locked.
    fn hold(&self, count: usize) {
        self.holding.store(count, std::sync::atomic::Ordering::SeqCst);
    }

    /// Rings, then waits at the door if this call is one of the held ones.
    async fn pass(&self) {
        // Ignored: a test that has stopped listening is a test that has what
        // it came for, and the call still has to get past the door.
        let _ = self.arrivals.send(());

        let held = self.holding.fetch_update(
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
            |left| left.checked_sub(1),
        );

        if held.is_err() {
            return;
        }

        let opened = self.door.lock().await;
        drop(opened);
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

// ---------------------------------------------------------------------------
// A transition outlives the request that asked for it.
//
// Recording the integration and settling the live binding on it are one change
// written to two places, and the second half waits for the binding to drain.
// Run inside an operator's request that wait is cancellable, and a
// cancellation between the halves leaves the record naming one repository and
// the platform reading another — a split nothing notices until a restart.
//
// What these pin is that no request can produce that split any more, however
// it goes away, and that two of them overlapping cannot either.
// ---------------------------------------------------------------------------

/// Waits for the detached transition to reach the target.
///
/// The transition deliberately runs in a task the service does not hand back —
/// that is the property under test — so there is nothing for a test to join.
/// Polling with a ceiling is what is left, and it is enough: it settles in
/// milliseconds when the transition runs, and gives up in bounded time when it
/// does not, which is what the implementation before it would have done.
async fn bound_at_least(target: &RecordingTarget, count: usize) {
    for _ in 0..1_000 {
        if target.bindings().len() >= count {
            return;
        }

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    panic!("a cancelled request left the binding unsettled");
}

/// Waits for the record to go, which is the last thing a disconnect does.
async fn forgotten(store: &InMemoryIntegrationStore, kind: IntegrationKind) {
    for _ in 0..1_000 {
        if store.load(kind).await.expect("readable").is_none() {
            return;
        }

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    panic!("a cancelled request left the record behind");
}

/// What a gated test drives besides the service itself.
struct Gated {
    /// The stores the flow was built over.
    harness: Harness,

    /// What it was pointed at, and what it is parked in.
    target: Arc<RecordingTarget>,

    /// The door to hold, and the bell to listen to.
    gate: Arc<Gate>,

    /// Rung once for every call that reaches the door.
    arrivals: tokio::sync::mpsc::UnboundedReceiver<()>,
}

/// A platform integration connected to `FieldstateNZ/first`, over a gated target.
async fn gated(repositories: &[(&str, &str)], first: &str) -> (Arc<GitIntegrationService>, Gated) {
    let (gate, mut arrivals) = Gate::new();
    let harness = harness(repositories);
    let target = RecordingTarget::gated(&gate);
    let service = Arc::new(platform_flow(&harness, &target));

    connect(&service, "FieldstateNZ", first).await;

    // Connecting went through the gate too, and its bell is still ringing.
    while arrivals.try_recv().is_ok() {}

    (
        service,
        Gated {
            harness,
            target,
            gate,
            arrivals,
        },
    )
}

#[tokio::test]
async fn a_cancelled_rebind_still_settles_the_binding_on_the_stored_repository() {
    let (service, mut gated) = gated(&[("FieldstateNZ", "a"), ("FieldstateNZ", "b")], "a").await;

    // The next call into the target stops there until this guard is dropped.
    let door = Arc::clone(&gated.gate.door).lock_owned().await;
    gated.gate.hold(1);

    let request = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.choose_repository(&operator(), "FieldstateNZ", "b").await }
    });

    gated.arrivals.recv().await.expect("the bind must be entered");

    // The request timeout firing, or the operator's browser going away. Before
    // the transition task this dropped the future the bind was running in, so
    // the record said `b` and the platform went on reading `a` — with nothing
    // to report it and nothing to repair it short of a restart.
    request.abort();
    assert!(request.await.is_err(), "the request is gone, mid-bind");

    drop(door);

    bound_at_least(&gated.target, 2).await;

    assert_eq!(
        gated.target.bindings(),
        vec!["FieldstateNZ/a".to_owned(), "FieldstateNZ/b".to_owned()],
        "the transition must settle the binding on what it stored"
    );
    assert_eq!(
        service
            .current()
            .await
            .expect("readable")
            .expect("recorded")
            .repository()
            .map(SelectedRepository::describe),
        Some("FieldstateNZ/b".to_owned()),
        "and the record must name the same one"
    );
}

#[tokio::test]
async fn a_cancelled_disconnect_still_clears_the_key_and_the_record() {
    let (service, mut gated) = gated(&[("FieldstateNZ", "a"), ("FieldstateNZ", "b")], "a").await;
    let released = gated.target.releases();

    let door = Arc::clone(&gated.gate.door).lock_owned().await;
    gated.gate.hold(1);

    let request = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.disconnect(&operator()).await }
    });

    gated.arrivals.recv().await.expect("the unbind must be entered");

    // The window the old rustdoc denied existed. Cut off here — after the
    // drain has begun and before either deletion — a disconnect used to leave
    // the binding on its way to released with the key and the record still
    // there, which is the opposite of "nothing has been released".
    request.abort();
    assert!(request.await.is_err(), "the request is gone, mid-unbind");

    drop(door);

    forgotten(&gated.harness.store, IntegrationKind::PlatformManagement).await;

    assert!(
        gated
            .harness
            .secrets
            .get(&SecretName::new(
                IntegrationKind::PlatformManagement.private_key()
            ))
            .await
            .expect("readable")
            .is_none(),
        "the key must go with the record; a key nothing accounts for is the worse half to keep"
    );
    assert_eq!(
        gated.target.releases(),
        released + 1,
        "and the binding must actually have been released"
    );
}

#[tokio::test]
async fn two_transitions_settle_on_the_last_one_asked_for() {
    let (service, mut gated) = gated(
        &[
            ("FieldstateNZ", "a"),
            ("FieldstateNZ", "b"),
            ("FieldstateNZ", "c"),
        ],
        "a",
    )
    .await;

    // The *first* call into the target and no other. The second transition has
    // to be free to run straight through to its own bind, because interleaving
    // is what this test is about: save `b`, save `c`, bind `c`, bind `b` leaves
    // the record naming `c` and the platform reading `b`.
    let door = Arc::clone(&gated.gate.door).lock_owned().await;
    gated.gate.hold(1);

    let first = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.choose_repository(&operator(), "FieldstateNZ", "b").await }
    });

    gated
        .arrivals
        .recv()
        .await
        .expect("the first bind must be entered");

    let second = tokio::spawn({
        let service = Arc::clone(&service);
        async move { service.choose_repository(&operator(), "FieldstateNZ", "c").await }
    });

    // Spawned while the first is parked inside the target, so the second is
    // genuinely overlapping it. Long enough for it to get as far as it is
    // going to get: past the door if nothing holds it back, and up against the
    // order if something does.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    drop(door);

    first
        .await
        .expect("the first request must finish")
        .expect("and be accepted");
    second
        .await
        .expect("the second request must finish")
        .expect("and be accepted");

    assert_eq!(
        gated.target.bindings(),
        vec![
            "FieldstateNZ/a".to_owned(),
            "FieldstateNZ/b".to_owned(),
            "FieldstateNZ/c".to_owned()
        ],
        "the order is taken inside the task and held across the whole transition, so each applies \
         in full and in the order it reached the lock — never a save of `c` settled by a bind of `b`"
    );
    assert_eq!(
        service
            .current()
            .await
            .expect("readable")
            .expect("recorded")
            .repository()
            .map(SelectedRepository::describe),
        Some("FieldstateNZ/c".to_owned()),
        "and the record and the binding must agree on the last one asked for"
    );
}
