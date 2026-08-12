# fabric-tenant-runtime — LLM context

The runtime tenant registry. Event-ID domain **2**. Depends on `fabric-core`,
`fabric-connector`, `arc-swap`, `async-trait`, `tokio` (fs/sync/time/rt),
`serde`, `serde_json`, `thiserror`, `tracing`.

## Public surface

- `TenantRuntimeRegistry`
  - `new()` — **unprimed**; holds no snapshot (not an empty one).
  - `resolve(&TenantId) -> Result<Arc<TenantRuntimeBinding>, ResolveError>` —
    request path. `ArcSwapOption::load` + hash lookup. No I/O, no locks.
  - `apply_all(Vec<TenantRuntimeBinding>) -> ApplyReport` — **full sync**;
    absent tenants removed. Per-tenant: older revision → `stale_ignored`,
    equal → `unchanged`, newer → `updated`, new → `added`.
  - `apply_one(TenantRuntimeBinding) -> bool` — incremental; `false` if not newer.
  - `invalidate(&TenantId) -> bool`.
  - `subscribe() -> broadcast::Receiver<BindingChange>`.
  - `is_primed()`, `len()`, `is_empty()`.
- `ApplyReport { added, updated, removed, stale_ignored, unchanged }`, `is_noop()`.
- `TenantRuntimeBinding { tenant, revision, data, configuration, secrets, features, storage }`
  - `new()`, `with_data()`, `data_source()`, **`execution_target(&DataSourceName)`**,
    `feature(&str)`. Serde `deny_unknown_fields`.
- `DataBinding { connector: ConnectorId, connection: ConnectionSelector, isolation: IsolationModel }`.
- `ConfigurationBinding`, `StorageBinding` — placeholders for §27 sibling APIs.
- `BindingChange { tenant, kind, previous_revision, current_revision }`,
  `BindingChangeKind::{Added, Updated, Removed}`, constructors `added/updated/removed`.
- `ResolveError::{RuntimeUnavailable, UnknownTenant, UnknownDataSource{tenant, data_source}}`.
- `BindingSourceError::{Unreadable{origin, cause}, Malformed{origin, detail}}` —
  field is `origin`, **not** `source` (thiserror reserves `source`).
- `BindingSource` (async trait) — `load() -> Result<Vec<_>, _>`, `describe()`.
- `InMemoryBindingSource` — `new/empty/set/fail_next`. `FileBindingSource::new(path)` — JSON array.
- `BindingRefresher::prime(&registry, &source)`, `::spawn(registry, source, config) -> RefreshHandle`.
- `RefreshHandle::refresh_now()`, `.shutdown().await`.
- `TenantRuntimeConfig { refresh_interval_seconds (30), fail_fast_on_prime (true) }`, `validate()`.
- `build_tenant_runtime(&config, source) -> Result<(Arc<Registry>, RefreshHandle), String>`.

## Internal

- `snapshot::RegistrySnapshot` — `HashMap<TenantId, Arc<TenantRuntimeBinding>>`,
  `pub(crate)`. Whole-snapshot swap, never mutated in place.

## Hard invariants — do not break

1. **Unprimed ≠ empty.** `RuntimeUnavailable` (503) must stay distinguishable
   from `UnknownTenant`. Conflating them breaks cold starts.
2. **A load failure never touches the registry.** `Err` → keep last good
   snapshot. Never convert a read error into `Ok(vec![])`.
3. **Revisions only move forward.** Never apply an older revision.
4. **No control-plane access from a `BindingSource`.** No Git, no Kubernetes
   API. (§6)
5. **No fallback on resolution failure.** No default tenant, no first-available
   database, no shared connection. (§28)
6. **Publish change events after the snapshot swap**, so a subscriber that
   reacts by resolving sees the new state.
7. Physical binding detail never escapes upward — only `ExecutionTarget`, and
   that goes *down* to a connector.

## Notes

- `apply_all` asymmetry: staleness is per tenant, removal is taken at face
  value. Documented trade-off; it is what makes deprovisioning possible.
- `refresh_now()` coalesces bursts via `Notify::notify_one`.
