//! A minimal resource for exercising the registry lifecycle.
//!
//! A purpose-built fixture rather than `TenantRuntimeBinding` or `DataSource`:
//! what is under test is the *lifecycle*, and using a real domain type would
//! mix its own validation rules into every assertion.

use fabric_core::BindingRevision;

use crate::resource::{RegistryResource, ResourceRegistry};
use crate::ConfigurationError;

/// The smallest thing a registry can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestResource {
    pub(super) key: String,
    pub(super) revision: BindingRevision,
    /// Content beyond key and revision, so a test can construct two
    /// resources that share both but differ in payload — the shape item
    /// 50's divergent-payload guard exists to catch.
    pub(super) payload: &'static str,
    /// Whether [`RegistryResource::validate`] accepts this resource. A plain
    /// switch rather than a real rule, so the load-path validation tests
    /// exercise the *lifecycle's* handling of a rejection without inheriting
    /// a domain type's own notion of what makes it invalid.
    pub(super) valid: bool,
}

impl RegistryResource for TestResource {
    type Key = String;

    const KIND: &'static str = "test resource";

    fn key(&self) -> &Self::Key {
        &self.key
    }

    fn revision(&self) -> BindingRevision {
        self.revision
    }

    fn validate(&self) -> Result<(), ConfigurationError> {
        if self.valid {
            return Ok(());
        }

        Err(ConfigurationError::InvalidResource(format!(
            "test resource {} is deliberately invalid",
            self.key
        )))
    }
}

/// A resource with the given key and revision, and a fixed default payload.
pub(super) fn resource(key: &str, revision: u64) -> TestResource {
    resource_with_payload(key, revision, "default")
}

/// A resource with an explicit payload, for exercising the divergent-payload
/// guard: two resources sharing a key and revision but not a payload.
pub(super) fn resource_with_payload(key: &str, revision: u64, payload: &'static str) -> TestResource {
    TestResource {
        key: key.to_owned(),
        revision: BindingRevision::new(revision),
        payload,
        valid: true,
    }
}

/// A resource that fails validation, for exercising the load-path check.
pub(super) fn invalid_resource(key: &str, revision: u64) -> TestResource {
    TestResource {
        valid: false,
        ..resource(key, revision)
    }
}

/// An empty registry of test resources.
pub(super) fn registry() -> ResourceRegistry<TestResource> {
    ResourceRegistry::new()
}
