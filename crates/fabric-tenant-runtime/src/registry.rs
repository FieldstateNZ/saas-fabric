//! The registry itself: lookup, apply, invalidate.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use fabric_core::TenantId;
use tokio::sync::broadcast;

use crate::snapshot::RegistrySnapshot;
use crate::{logging, BindingChange, ResolveError, TenantRuntimeBinding};

/// How many change notifications are buffered for a slow subscriber before it
/// starts losing the oldest.
///
/// A lagging subscriber is told it lagged and can re-read current state from
/// the registry, so losing events is recoverable. Blocking the registry on a
/// slow subscriber would not be.
const CHANGE_CHANNEL_CAPACITY: usize = 256;

/// What one call to [`TenantRuntimeRegistry::apply_all`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Tenants the registry had not seen before.
    pub added: usize,
    /// Tenants whose binding advanced to a newer revision.
    pub updated: usize,
    /// Tenants that disappeared from the source.
    pub removed: usize,
    /// Tenants whose incoming binding was **older** than the one held, and was
    /// therefore ignored.
    pub stale_ignored: usize,
    /// Tenants whose incoming binding matched what was already held.
    pub unchanged: usize,
}

impl ApplyReport {
    /// Whether anything actually moved.
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        self.added == 0 && self.updated == 0 && self.removed == 0
    }
}

/// The runtime plane's view of every tenant.
///
/// Reads are lock-free: [`Self::resolve`] is an atomic pointer load and a hash
/// lookup. Writes build a new snapshot and swap it, so a refresh never stalls a
/// request (see [`RegistrySnapshot`](crate::snapshot::RegistrySnapshot)).
///
/// # Priming
///
/// A fresh registry holds **no snapshot at all**, which is different from
/// holding an empty one. Until the first successful load, every resolution
/// returns [`ResolveError::RuntimeUnavailable`] — a 503 — rather than
/// [`ResolveError::UnknownTenant`]. Getting this distinction right matters:
/// during a cold start, telling every caller their tenant does not exist would
/// be both wrong and alarming.
pub struct TenantRuntimeRegistry {
    snapshot: ArcSwapOption<RegistrySnapshot>,
    changes: broadcast::Sender<BindingChange>,
}

impl Default for TenantRuntimeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TenantRuntimeRegistry {
    /// An unprimed registry. Resolves nothing until bindings are applied.
    #[must_use]
    pub fn new() -> Self {
        let (changes, _) = broadcast::channel(CHANGE_CHANNEL_CAPACITY);

        Self {
            snapshot: ArcSwapOption::empty(),
            changes,
        }
    }

    /// Resolves a tenant's current bindings.
    ///
    /// This is the request-path operation. No I/O, no locks, no allocation
    /// beyond an `Arc` clone.
    ///
    /// # Errors
    ///
    /// - [`ResolveError::RuntimeUnavailable`] if no snapshot has loaded yet.
    /// - [`ResolveError::UnknownTenant`] if the tenant is not in the snapshot.
    pub fn resolve(&self, tenant: &TenantId) -> Result<Arc<TenantRuntimeBinding>, ResolveError> {
        let guard = self.snapshot.load();

        let Some(snapshot) = guard.as_ref() else {
            logging::resolve_before_prime(tenant);
            return Err(ResolveError::RuntimeUnavailable);
        };

        snapshot.get(tenant).map(Arc::clone).ok_or_else(|| {
            logging::unknown_tenant(tenant);
            ResolveError::UnknownTenant(tenant.clone())
        })
    }

    /// Replaces the registry contents with an authoritative set of bindings.
    ///
    /// This is a **full sync**: a tenant absent from `bindings` is treated as
    /// deprovisioned and removed. That is correct for a source that publishes
    /// complete reconciled state, which is the model §6 describes.
    ///
    /// Per tenant, an incoming binding whose revision is *older* than the one
    /// held is ignored and counted in [`ApplyReport::stale_ignored`]. Revisions
    /// only move forward (§20), so an older revision means a stale read, and
    /// applying it would resurrect a retired binding — potentially pointing a
    /// tenant back at a database that a migration has just drained.
    ///
    /// Note the asymmetry: staleness is enforced per tenant, but removal is
    /// taken at face value. A source that publishes a *truncated* set will
    /// therefore remove tenants. That is the price of supporting deprovisioning
    /// at all, and it is why a load failure must never be turned into an empty
    /// set — see [`BindingSourceError`](crate::BindingSourceError).
    pub fn apply_all(&self, bindings: Vec<TenantRuntimeBinding>) -> ApplyReport {
        let guard = self.snapshot.load();
        let current = guard.as_ref();

        let mut report = ApplyReport::default();
        let mut next: HashMap<TenantId, Arc<TenantRuntimeBinding>> = HashMap::with_capacity(bindings.len());
        let mut events = Vec::new();

        for incoming in bindings {
            let existing = current.and_then(|snapshot| snapshot.get(&incoming.tenant));

            match existing {
                None => {
                    report.added += 1;
                    events.push(BindingChange::added(incoming.tenant.clone(), incoming.revision));
                    next.insert(incoming.tenant.clone(), Arc::new(incoming));
                }
                Some(held) if incoming.revision > held.revision => {
                    report.updated += 1;
                    events.push(BindingChange::updated(
                        incoming.tenant.clone(),
                        held.revision,
                        incoming.revision,
                    ));
                    next.insert(incoming.tenant.clone(), Arc::new(incoming));
                }
                Some(held) if incoming.revision < held.revision => {
                    report.stale_ignored += 1;
                    logging::stale_binding_ignored(&incoming.tenant, incoming.revision, held.revision);
                    next.insert(incoming.tenant.clone(), Arc::clone(held));
                }
                Some(held) => {
                    report.unchanged += 1;
                    next.insert(incoming.tenant.clone(), Arc::clone(held));
                }
            }
        }

        if let Some(snapshot) = current {
            for (tenant, held) in snapshot.tenants() {
                if !next.contains_key(tenant) {
                    report.removed += 1;
                    events.push(BindingChange::removed(tenant.clone(), held.revision));
                }
            }
        }

        let size = next.len();
        self.snapshot.store(Some(Arc::new(RegistrySnapshot::new(next))));

        // Published only after the swap, so a subscriber that reacts by
        // resolving the tenant sees the new state rather than the old.
        self.publish(events);
        logging::snapshot_applied(size, &report);

        report
    }

    /// Applies a single tenant's binding without touching the rest.
    ///
    /// Used for incremental updates — a reconciler notifying the runtime that
    /// one tenant has changed, rather than republishing everything.
    ///
    /// Returns `false` if the incoming revision is not newer than what is held,
    /// in which case nothing changes.
    ///
    /// On an unprimed registry this primes it with a single tenant. That is
    /// intentional for tests and for incremental-only deployments, but a
    /// production start should [`Self::apply_all`] first so that resolution
    /// does not report every other tenant as unknown.
    pub fn apply_one(&self, binding: TenantRuntimeBinding) -> bool {
        let guard = self.snapshot.load();

        let mut next = guard
            .as_ref()
            .map(|snapshot| snapshot.tenants().clone())
            .unwrap_or_default();

        let event = match next.get(&binding.tenant) {
            Some(held) if binding.revision <= held.revision => {
                logging::stale_binding_ignored(&binding.tenant, binding.revision, held.revision);
                return false;
            }
            Some(held) => BindingChange::updated(binding.tenant.clone(), held.revision, binding.revision),
            None => BindingChange::added(binding.tenant.clone(), binding.revision),
        };

        next.insert(binding.tenant.clone(), Arc::new(binding));
        self.snapshot.store(Some(Arc::new(RegistrySnapshot::new(next))));
        self.publish(vec![event]);

        true
    }

    /// Drops one tenant's binding.
    ///
    /// Returns `false` if the tenant was not held. After this, resolving that
    /// tenant fails closed until the next refresh restores it — which is the
    /// intended behaviour for a deprovisioned tenant, and an acceptable
    /// momentary outage for a mistakenly invalidated one.
    pub fn invalidate(&self, tenant: &TenantId) -> bool {
        let guard = self.snapshot.load();

        let Some(snapshot) = guard.as_ref() else {
            return false;
        };

        let Some(held) = snapshot.get(tenant) else {
            return false;
        };

        let revision = held.revision;
        let mut next = snapshot.tenants().clone();
        next.remove(tenant);

        self.snapshot.store(Some(Arc::new(RegistrySnapshot::new(next))));
        self.publish(vec![BindingChange::removed(tenant.clone(), revision)]);

        true
    }

    /// Subscribes to binding transitions.
    ///
    /// See [`BindingChange`] for what to do about them, and note that a slow
    /// subscriber lags rather than blocking the registry.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<BindingChange> {
        self.changes.subscribe()
    }

    /// Whether a snapshot has ever been loaded.
    ///
    /// Drives the readiness probe: a process that has not primed cannot serve
    /// any tenant and should not receive traffic.
    #[must_use]
    pub fn is_primed(&self) -> bool {
        self.snapshot.load().is_some()
    }

    /// How many tenants are currently held. Zero when unprimed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshot.load().as_ref().map_or(0, |snapshot| snapshot.len())
    }

    /// Whether the registry holds no tenants.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Broadcasts change events, tolerating the common case of no subscribers.
    fn publish(&self, events: Vec<BindingChange>) {
        for event in events {
            // `send` fails only when nobody is listening, which is normal at
            // startup and in tests. There is nothing to recover from.
            drop(self.changes.send(event));
        }
    }
}

#[cfg(test)]
mod tests {
    use fabric_connector::{ConnectionSelector, ConnectorId, IsolationModel};
    use fabric_core::{BindingRevision, DataSourceName};

    use super::*;
    use crate::{BindingChangeKind, DataBinding};

    fn tenant(name: &str) -> TenantId {
        TenantId::try_new(name).unwrap()
    }

    fn binding_at(name: &str, revision: u64) -> TenantRuntimeBinding {
        TenantRuntimeBinding::new(tenant(name), BindingRevision::new(revision)).with_data(
            DataSourceName::try_new("primary").unwrap(),
            DataBinding {
                connector: ConnectorId::try_new("postgres").unwrap(),
                connection: ConnectionSelector::Default,
                isolation: IsolationModel::Database,
            },
        )
    }

    #[test]
    fn an_unprimed_registry_reports_unavailable_not_unknown_tenant() {
        // The distinction is the whole point: a cold start must not tell every
        // caller their tenant has been deleted.
        let registry = TenantRuntimeRegistry::new();

        assert_eq!(
            registry.resolve(&tenant("acme")).unwrap_err(),
            ResolveError::RuntimeUnavailable
        );
        assert!(!registry.is_primed());
    }

    #[test]
    fn a_primed_registry_reports_an_absent_tenant_as_unknown() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 1)]);

        assert_eq!(
            registry.resolve(&tenant("globex")).unwrap_err(),
            ResolveError::UnknownTenant(tenant("globex"))
        );
    }

    #[test]
    fn priming_with_an_empty_set_still_counts_as_primed() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![]);

        assert!(registry.is_primed());
        assert_eq!(
            registry.resolve(&tenant("acme")).unwrap_err(),
            ResolveError::UnknownTenant(tenant("acme"))
        );
    }

    #[test]
    fn resolves_a_held_binding() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 7)]);

        let resolved = registry.resolve(&tenant("acme")).unwrap();
        assert_eq!(resolved.revision, BindingRevision::new(7));
    }

    #[test]
    fn a_full_sync_removes_tenants_absent_from_the_incoming_set() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 1), binding_at("globex", 1)]);

        let report = registry.apply_all(vec![binding_at("acme", 1)]);

        assert_eq!(report.removed, 1);
        assert!(registry.resolve(&tenant("globex")).is_err());
    }

    #[test]
    fn an_older_revision_is_ignored_rather_than_resurrecting_a_retired_binding() {
        // A stale read must not point a tenant back at a database that a
        // migration has already drained.
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 10)]);

        let report = registry.apply_all(vec![binding_at("acme", 3)]);

        assert_eq!(report.stale_ignored, 1);
        assert_eq!(
            registry.resolve(&tenant("acme")).unwrap().revision,
            BindingRevision::new(10)
        );
    }

    #[test]
    fn an_identical_revision_is_reported_as_unchanged() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 5)]);

        let report = registry.apply_all(vec![binding_at("acme", 5)]);

        assert_eq!(report.unchanged, 1);
        assert!(report.is_noop());
    }

    #[test]
    fn applying_one_tenant_leaves_the_others_alone() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 1), binding_at("globex", 1)]);

        assert!(registry.apply_one(binding_at("acme", 2)));

        assert_eq!(
            registry.resolve(&tenant("acme")).unwrap().revision,
            BindingRevision::new(2)
        );
        assert_eq!(
            registry.resolve(&tenant("globex")).unwrap().revision,
            BindingRevision::new(1)
        );
    }

    #[test]
    fn applying_one_tenant_at_the_same_revision_is_refused() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 4)]);

        assert!(!registry.apply_one(binding_at("acme", 4)));
    }

    #[test]
    fn invalidating_a_tenant_makes_it_fail_closed() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 1)]);

        assert!(registry.invalidate(&tenant("acme")));
        assert!(registry.resolve(&tenant("acme")).is_err());
        // The registry is still primed — one tenant went away, the plane is up.
        assert!(registry.is_primed());
    }

    #[test]
    fn invalidating_an_absent_tenant_reports_that_nothing_happened() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![]);

        assert!(!registry.invalidate(&tenant("acme")));
    }

    #[tokio::test]
    async fn subscribers_are_told_when_a_binding_advances() {
        let registry = TenantRuntimeRegistry::new();
        let mut changes = registry.subscribe();

        registry.apply_all(vec![binding_at("acme", 1)]);
        registry.apply_one(binding_at("acme", 2));

        let added = changes.recv().await.unwrap();
        assert_eq!(added.kind, BindingChangeKind::Added);
        assert_eq!(added.current_revision, Some(BindingRevision::new(1)));

        let updated = changes.recv().await.unwrap();
        assert_eq!(updated.kind, BindingChangeKind::Updated);
        assert_eq!(updated.previous_revision, Some(BindingRevision::new(1)));
        assert_eq!(updated.current_revision, Some(BindingRevision::new(2)));
    }

    #[tokio::test]
    async fn subscribers_are_told_when_a_tenant_is_removed() {
        let registry = TenantRuntimeRegistry::new();
        registry.apply_all(vec![binding_at("acme", 3)]);

        let mut changes = registry.subscribe();
        registry.invalidate(&tenant("acme"));

        let removed = changes.recv().await.unwrap();
        assert_eq!(removed.kind, BindingChangeKind::Removed);
        assert_eq!(removed.previous_revision, Some(BindingRevision::new(3)));
        assert_eq!(removed.current_revision, None);
    }

    #[tokio::test]
    async fn a_change_is_published_only_after_the_new_state_is_visible() {
        // A subscriber that reacts by resolving must not see the old binding.
        let registry = Arc::new(TenantRuntimeRegistry::new());
        registry.apply_all(vec![binding_at("acme", 1)]);

        let mut changes = registry.subscribe();
        registry.apply_one(binding_at("acme", 2));

        let change = changes.recv().await.unwrap();
        assert_eq!(change.current_revision, Some(BindingRevision::new(2)));
        assert_eq!(
            registry.resolve(&tenant("acme")).unwrap().revision,
            BindingRevision::new(2)
        );
    }
}
