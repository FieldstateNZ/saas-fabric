//! A minimal resource for exercising the registry lifecycle.
//!
//! A purpose-built fixture rather than `TenantRuntimeBinding` or `DataSource`:
//! what is under test is the *lifecycle*, and using a real domain type would
//! mix its own validation rules into every assertion.

use fabric_core::BindingRevision;

use crate::resource::{RegistryResource, ResourceRegistry};

/// The smallest thing a registry can hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestResource {
    pub(super) key: String,
    pub(super) revision: BindingRevision,
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
}

/// A resource with the given key and revision.
pub(super) fn resource(key: &str, revision: u64) -> TestResource {
    TestResource {
        key: key.to_owned(),
        revision: BindingRevision::new(revision),
    }
}

/// An empty registry of test resources.
pub(super) fn registry() -> ResourceRegistry<TestResource> {
    ResourceRegistry::new()
}
