//! Not connected is a state, and a connected-but-broken one is not it.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::PlatformDesiredState;
use crate::{ComponentDesired, DesiredState, DesiredStateError, ReleaseUnit};

/// A repository that is reachable, and whose reads fail.
struct Connected;

#[async_trait::async_trait]
impl DesiredState for Connected {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(vec!["saas-fabric".to_owned()])
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        Err(DesiredStateError::Unavailable {
            detail: "the platform repository timed out".to_owned(),
        })
    }

    async fn advance(&self, _: &str, _: &str, _: &ReleaseUnit, _: &str) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

fn unit() -> ReleaseUnit {
    ReleaseUnit {
        version: crate::Version::parse("0.3.0-preview.1").expect("a version"),
        source_revision: "abc".to_owned(),
        images: BTreeMap::new(),
    }
}

#[tokio::test]
async fn every_operation_says_not_connected_until_something_is() {
    let binding = PlatformDesiredState::unconnected();

    assert!(!binding.is_connected());
    assert_eq!(
        binding
            .components("lucentroot")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
    assert_eq!(
        binding
            .component("lucentroot", "saas-fabric")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
    assert_eq!(
        binding
            .advance("lucentroot", "saas-fabric", &unit(), "Promote")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
}

#[tokio::test]
async fn connecting_something_makes_it_answer() {
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::new(Connected));

    assert!(binding.is_connected());
    assert_eq!(
        binding.components("lucentroot").await.unwrap(),
        vec!["saas-fabric"]
    );
}

#[tokio::test]
async fn a_connected_repository_that_fails_does_not_look_disconnected() {
    // The distinction an operator's next step depends on. "Not connected"
    // sends them to connect one; this one is already connected and broken, and
    // saying otherwise sends them to do something they have already done.
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::new(Connected));

    let failure = binding
        .component("lucentroot", "saas-fabric")
        .await
        .expect_err("this repository fails that call");

    assert_ne!(failure, DesiredStateError::NotConnected);
    assert!(matches!(failure, DesiredStateError::Unavailable { .. }));
}

#[tokio::test]
async fn disconnecting_goes_back_to_not_connected() {
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Arc::new(Connected));
    binding.disconnect();

    assert!(!binding.is_connected());
    assert_eq!(
        binding.components("lucentroot").await.expect_err("disconnected"),
        DesiredStateError::NotConnected
    );
}
