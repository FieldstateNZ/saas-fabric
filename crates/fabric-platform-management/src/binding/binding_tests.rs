//! Not connected is a state, and a connected-but-broken one is not it.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::PlatformDesiredState;
use crate::{ComponentDesired, DesiredState, DesiredStateError, Release, ReleaseUnit};

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

    async fn advance(&self, _: &str, _: &str, _: &Release, _: &str) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &ReleaseUnit,
        _: &crate::Hold,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn pause(&self, _: &str, _: &str, _: &crate::Hold, _: &str) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn resume(&self, _: &str, _: &str, _: &str) -> Result<(), DesiredStateError> {
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
            .advance("lucentroot", "saas-fabric", &Release::Unit(unit()), "Promote")
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

#[tokio::test]
async fn an_integration_that_could_not_be_built_is_failing_rather_than_absent() {
    // Somebody connected this. "Nothing is connected" would send them to
    // connect it a second time instead of to the reason the first one stopped
    // working — which is the failure this whole three-state binding exists to
    // prevent.
    let binding = PlatformDesiredState::unconnected();

    binding.unusable("the application's key could not be read");

    let failure = binding
        .components("lucentroot")
        .await
        .expect_err("a repository that could not be built cannot be read");

    assert_ne!(failure, DesiredStateError::NotConnected);
    assert!(
        matches!(failure, DesiredStateError::Unavailable { detail } if detail.contains("key")),
        "an operator is told what went wrong, in words that are safe to show"
    );
}

#[tokio::test]
async fn connecting_after_a_failure_replaces_it() {
    // The recovery path: the key comes back, the next restore binds, and
    // nothing is left saying the integration is broken.
    let binding = PlatformDesiredState::unconnected();

    binding.unusable("the application's key could not be read");
    binding.connect(Arc::new(Connected));

    assert!(binding.is_connected());
    assert_eq!(
        binding
            .components("lucentroot")
            .await
            .expect("the repository is connected again"),
        vec!["saas-fabric".to_owned()]
    );
}

#[tokio::test]
async fn disconnecting_after_a_failure_goes_back_to_not_connected() {
    // An operator who gives up and forgets the integration has genuinely not
    // connected one, and should be told so rather than shown a stale failure.
    let binding = PlatformDesiredState::unconnected();

    binding.unusable("the application's key could not be read");
    binding.disconnect();

    assert_eq!(
        binding
            .components("lucentroot")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
}
