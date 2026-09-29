//! Which client the connector builds for a connection, and what it installs.
//! Nothing here sends a request.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use fabric_control_plane::{
    Readability, RealmOrigin, RegistryClient, RegistryConnection, RegistryConnector, RegistryCredential,
    RegistryHost, RegistryKind, Repository, SecretValue,
};
use fabric_platform_management::RegistryError;
use fabric_registry::{AddressPolicy, RealmRule, Registries};

use super::connector::Connector;
use super::deployment::Deployment;
use crate::config::RegistryBinding;

fn connection(host: &str, kind: RegistryKind, endpoint: &str, credential: bool) -> RegistryConnection {
    RegistryConnection {
        host: RegistryHost::parse(host).unwrap(),
        kind,
        endpoint: endpoint.to_owned(),
        realm: RealmOrigin::Recorded(None),
        credential: credential.then(|| RegistryCredential::new("registry-robot", SecretValue::new("ghp_x"))),
        repositories: vec![Repository::try_new(format!("{host}/acme/app")).unwrap()],
    }
}

fn connector(deployment: Option<Deployment>) -> (Connector, Arc<Registries>) {
    let router = Arc::new(Registries::new(BTreeMap::new()));
    (Connector::new(Arc::clone(&router), deployment, 10), router)
}

#[test]
fn each_kind_is_built_and_what_is_built_is_what_is_installed() {
    let (connector, router) = connector(None);

    let ghcr = connector
        .connect(connection("ghcr.io", RegistryKind::Ghcr, "https://ghcr.io", true))
        .unwrap();
    let hub = connector
        .connect(connection(
            "docker.io",
            RegistryKind::DockerHub,
            "https://registry-1.docker.io",
            false,
        ))
        .unwrap();
    let own = connector
        .connect(connection(
            "registry.example.com",
            RegistryKind::Distribution,
            "https://registry.example.com",
            false,
        ))
        .unwrap();

    connector.install(BTreeMap::from([
        (RegistryHost::parse("ghcr.io").unwrap(), ghcr),
        (RegistryHost::parse("docker.io").unwrap(), hub),
        (RegistryHost::parse("registry.example.com").unwrap(), own),
    ]));

    for host in ["ghcr.io", "docker.io", "registry.example.com"] {
        assert!(router.get(host).is_some(), "{host} was not installed");
    }
    assert!(router.get("quay.io").is_none(), "no default is ever installed");
}

#[test]
fn a_distribution_endpoint_the_adapter_refuses_is_not_built() {
    let (connector, _) = connector(None);

    let refused = connector.connect(connection(
        "registry.example.com",
        RegistryKind::Distribution,
        "http://registry.example.com",
        false,
    ));

    assert!(refused.is_err());
}

#[test]
fn the_deployments_registry_stays_installed_anonymously_until_its_host_is_registered() {
    let deployment = Deployment::build(&RegistryBinding::default()).unwrap();
    let anonymous = Arc::clone(&deployment.anonymous);
    let (connector, router) = connector(Some(deployment));

    connector.install(BTreeMap::new());
    assert!(Arc::ptr_eq(&router.get("ghcr.io").unwrap(), &anonymous));

    let credentialed = connector
        .connect(connection("ghcr.io", RegistryKind::Ghcr, "https://ghcr.io", true))
        .unwrap();
    connector.install(BTreeMap::from([(
        RegistryHost::parse("ghcr.io").unwrap(),
        credentialed,
    )]));
    assert!(!Arc::ptr_eq(&router.get("ghcr.io").unwrap(), &anonymous));

    connector.install(BTreeMap::new());
    assert!(
        Arc::ptr_eq(&router.get("ghcr.io").unwrap(), &anonymous),
        "removing it restores the anonymous default"
    );
}

#[test]
fn a_credential_for_the_deployments_host_keeps_its_kinds_realm_on_the_deployments_network() {
    let deployment = Deployment::build(&RegistryBinding::default()).unwrap();
    let (connector, _) = connector(Some(deployment));

    let (settings, _) = connector
        .settings(&connection(
            "ghcr.io",
            RegistryKind::Ghcr,
            "https://ghcr.io",
            true,
        ))
        .unwrap();

    assert_eq!(
        settings.realm(),
        &RealmRule::Fixed {
            realm: "https://ghcr.io/token".to_owned(),
            service: "ghcr.io".to_owned(),
        },
        "never whatever the first challenge after a restart names"
    );
    assert_eq!(settings.address(), AddressPolicy::Any);
}

#[test]
fn a_recorded_realm_is_held_at_the_deployments_host_too() {
    let binding = RegistryBinding {
        base_url: "https://registry.internal.example".to_owned(),
        host: "registry.internal.example".to_owned(),
        ..RegistryBinding::default()
    };
    let (connector, _) = connector(Some(Deployment::build(&binding).unwrap()));
    let mut recorded = connection(
        "registry.internal.example",
        RegistryKind::Distribution,
        "https://registry.internal.example",
        true,
    );
    recorded.realm = RealmOrigin::Recorded(Some("https://auth.internal.example".to_owned()));

    let (settings, _) = connector.settings(&recorded).unwrap();

    assert_eq!(
        settings.realm(),
        &RealmRule::Recorded {
            origin: Some("https://auth.internal.example".to_owned())
        }
    );
    assert_eq!(settings.address(), AddressPolicy::Any);
}

#[test]
fn a_credential_for_the_deployments_host_never_moves_to_another_endpoint() {
    let mirror = RegistryBinding {
        base_url: "https://mirror.internal.example".to_owned(),
        ..RegistryBinding::default()
    };
    let (connector, _) = connector(Some(Deployment::build(&mirror).unwrap()));

    let refused = connector.connect(connection("ghcr.io", RegistryKind::Ghcr, "https://ghcr.io", true));

    assert!(refused.err().unwrap().contains("base_url"));
}

/// A client some other connector built.
struct Foreign;

#[async_trait]
impl RegistryClient for Foreign {
    async fn prove(&self) -> Result<Option<String>, RegistryError> {
        Ok(None)
    }

    async fn prove_repository(&self, _repository: &Repository) -> Result<Readability<()>, RegistryError> {
        Ok(Readability::Readable(()))
    }

    async fn version_tags(
        &self,
        _repository: &Repository,
    ) -> Result<Readability<Vec<String>>, RegistryError> {
        Ok(Readability::Readable(Vec::new()))
    }

    fn credential_refused(&self) -> bool {
        false
    }
}

#[test]
fn a_client_this_connector_did_not_build_is_not_installed() {
    let (connector, router) = connector(None);

    connector.install(BTreeMap::from([(
        RegistryHost::parse("ghcr.io").unwrap(),
        Arc::new(Foreign) as Arc<dyn RegistryClient>,
    )]));

    assert!(router.get("ghcr.io").is_none());
}

#[tokio::test]
async fn a_zero_registry_timeout_is_refused_at_startup() {
    let composed = super::compose_registries(super::RegistryComposition {
        deployment: None,
        http_timeout_seconds: 0,
        store: Arc::new(fabric_control_plane::InMemoryRegistryStore::new()),
        secrets: Arc::new(fabric_control_plane::InMemorySecretStore::new()),
        clock: fabric_core::SystemClock::shared(),
    })
    .await;

    assert!(composed.err().unwrap().contains("http_timeout_seconds"));
}
