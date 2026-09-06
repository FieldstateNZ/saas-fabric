//! An in-memory identity provider that behaves like a real one.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use fabric_client_model::RealmName;

use crate::provider::{ObservedRealm, ProviderError};

/// The audience [`FakeIdentityProvider::new`] asserts.
///
/// Most tests only care that a client written through the fake reads back as
/// converged, not what the audience string actually is —
/// [`FakeIdentityProvider::with_audience`] is for the ones that do.
const DEFAULT_AUDIENCE: &str = "saas-fabric-data-api";

/// An identity provider held entirely in memory.
pub struct FakeIdentityProvider {
    /// The realms that exist, and what is in them.
    pub(super) realms: Mutex<BTreeMap<RealmName, ObservedRealm>>,

    /// A failure to return from the next call, if one has been injected.
    failure: Mutex<Option<ProviderError>>,

    /// Every call made, in order, as `operation:argument` strings.
    calls: Mutex<Vec<String>>,

    /// The audience [`configured_audience`](crate::IdentityProvider::configured_audience)
    /// reports, and the audience every client written through this fake's
    /// mapper asserts.
    pub(super) audience: String,
}

impl FakeIdentityProvider {
    /// Builds a provider holding no realms, asserting `DEFAULT_AUDIENCE`.
    #[must_use]
    pub fn new() -> Self {
        Self::with_audience(DEFAULT_AUDIENCE)
    }

    /// Builds a provider holding no realms, asserting `audience`.
    ///
    /// Lets a test configure a fake that disagrees with a client's own
    /// desired-state audience — the scenario `new()` cannot express with one
    /// fixed constant.
    #[must_use]
    pub fn with_audience(audience: impl Into<String>) -> Self {
        Self {
            realms: Mutex::new(BTreeMap::new()),
            failure: Mutex::new(None),
            calls: Mutex::new(Vec::new()),
            audience: audience.into(),
        }
    }

    /// Makes every subsequent call fail.
    pub fn fail_with(&self, error: ProviderError) {
        *lock(&self.failure) = Some(error);
    }

    /// Stops failing.
    pub fn recover(&self) {
        *lock(&self.failure) = None;
    }

    /// Every call made so far, in order.
    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        lock(&self.calls).clone()
    }

    /// Forgets the calls made so far, so a second pass can be asserted on its
    /// own.
    pub fn clear_calls(&self) {
        lock(&self.calls).clear();
    }

    /// Replaces what the provider holds for a realm.
    ///
    /// The only way to express "something changed this realm from outside SaaS
    /// Fabric", which is the situation drift detection exists for and which no
    /// sequence of reconciler calls could produce.
    pub fn seed_realm(&self, realm: RealmName, state: ObservedRealm) {
        lock(&self.realms).insert(realm, state);
    }

    /// What the provider currently holds for a realm.
    #[must_use]
    pub fn realm(&self, realm: &RealmName) -> Option<ObservedRealm> {
        lock(&self.realms).get(realm).cloned()
    }

    /// Records a call and returns the injected failure, if any.
    pub(super) fn enter(&self, call: String) -> Result<(), ProviderError> {
        lock(&self.calls).push(call);

        lock(&self.failure).clone().map_or(Ok(()), Err)
    }
}

impl Default for FakeIdentityProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Takes a lock, recovering from a poisoned one rather than panicking.
pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
