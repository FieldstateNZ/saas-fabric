//! Each repository is read through the registry its host names, and a host
//! with none is refused rather than sent to a default.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_platform_management::{Registry, RegistryError};
use fabric_registry::{OciRegistry, Registries};
use support::{FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const NGINX: &str = "docker.io/library/nginx";

fn client(fake: &FakeRegistry, host: &str) -> Arc<OciRegistry> {
    Arc::new(OciRegistry::plain_http_to_loopback(&fake.base_url, host, 5).unwrap())
}

fn by_host(entries: &[(&str, &Arc<OciRegistry>)]) -> BTreeMap<String, Arc<OciRegistry>> {
    entries
        .iter()
        .map(|(host, client)| ((*host).to_owned(), Arc::clone(client)))
        .collect()
}

#[tokio::test]
async fn each_repository_is_read_through_its_hosts_registry() {
    let ghcr = FakeRegistry::start().await;
    let hub = FakeRegistry::start().await;
    ghcr.publish(RUNTIME, "0.3.0", "abc");
    hub.publish("library/nginx", "1.27.0", "def");
    let (to_ghcr, to_hub) = (client(&ghcr, HOST), client(&hub, "docker.io"));
    let registries = Registries::new(by_host(&[("ghcr.io", &to_ghcr), ("docker.io", &to_hub)]));

    assert_eq!(registries.tags(RUNTIME).await.unwrap(), vec!["0.3.0".to_owned()]);
    assert_eq!(registries.tags(NGINX).await.unwrap(), vec!["1.27.0".to_owned()]);
    assert!(registries.resolve(NGINX, "1.27.0").await.unwrap().is_some());
    assert!(ghcr.paths().iter().all(|path| !path.contains("nginx")));
    assert!(hub.paths().iter().all(|path| !path.contains("saas-fabric")));
    assert_eq!(registries.get("docker.io").unwrap().naming_host(), "docker.io");
}

#[tokio::test]
async fn a_host_with_no_registry_is_refused_naming_it() {
    let ghcr = FakeRegistry::start().await;
    let registries = Registries::new(by_host(&[("ghcr.io", &client(&ghcr, HOST))]));

    for repository in ["quay.io/team/app", "registry.example:5000/team/app"] {
        let failure = registries.tags(repository).await.expect_err("no registry");
        let RegistryError::Refused { detail } = failure else {
            panic!("expected Refused, got {failure:?}");
        };
        let host = repository.split('/').next().unwrap();
        assert!(detail.contains(host), "{detail}");
    }
    assert!(registries.tags("nginx").await.is_err(), "a name with no host");
    assert!(ghcr.requests().is_empty(), "nothing fell through to a default");
}

#[tokio::test]
async fn replacing_the_registries_replaces_them_all_at_once() {
    let ghcr = FakeRegistry::start().await;
    let hub = FakeRegistry::start().await;
    hub.publish("library/nginx", "1.27.0", "def");
    let registries = Registries::new(by_host(&[("ghcr.io", &client(&ghcr, HOST))]));
    assert!(registries.get("docker.io").is_none());

    registries.replace(by_host(&[("docker.io", &client(&hub, "docker.io"))]));

    assert!(registries.get("ghcr.io").is_none());
    assert!(matches!(
        registries.tags(RUNTIME).await,
        Err(RegistryError::Refused { .. })
    ));
    assert_eq!(registries.tags(NGINX).await.unwrap(), vec!["1.27.0".to_owned()]);
}
