# fabric-tenant-runtime — LLM context

The runtime plane. Event-ID domain **2**. Depends on `fabric-core`,
`fabric-connector`, `arc-swap`, `async-trait`, `tokio`, `serde`, `thiserror`,
`tracing`.

## The model (ADR 0003)

```
tenant → logical binding (primary) → DataSource → connector → infrastructure
```

Two independently reconciled resources. Tenant bindings hold **only** the
DataSource reference plus tenant-specific isolation. DataSources hold **all**
physical/provider concerns.

## Public surface

### Resolution — the only supported chain

- `RuntimeResolver::new(Arc<TenantRegistry>, Arc<DataSourceRegistry>)`.
  - `resolve_tenant(&TenantId) -> Result<Arc<TenantRuntimeBinding>, ResolveError>`
  - **`resolve_data_source(&TenantId, &LogicalDataSourceName) -> Result<ResolvedDataSource, ResolveError>`**
  - `tenants()`, `data_sources()`, `is_primed()` (conjunction of both).
- `ResolvedDataSource { target: ExecutionTarget, data_source: Arc<DataSource> }`,
  `is_writable()`, `telemetry_label()`.

### Resources

- `TenantRuntimeBinding { tenant, revision, data: BTreeMap<LogicalDataSourceName, TenantDataBinding>,
  configuration, secrets, features, storage }`. `new`, `with_data`,
  `data_binding()`, `feature()`, `validate()`. Serde `deny_unknown_fields`.
- `TenantDataBinding { data_source: DataSourceId, isolation: IsolationModel }`.
- `DataSource { id, revision, connector, connection, placement, residency, pool,
  capabilities, labels }`. `validate()`, `telemetry_label()`.
- `PlacementClass` — Shared, Dedicated, HighAvailability, Regulated,
  Development, Ephemeral. `as_str()`, `is_production()`.
- `DataResidency { region, jurisdiction }`. `in_region()`, `telemetry_label()`.
- `PoolSettings { max_connections (20), idle_timeout_seconds (300),
  acquire_timeout_seconds (5) }`. `validate()`. Applied by *reconciliation* to
  the connector, not by this process (ADR 0001).
- `DataSourceCapabilities { writable (true), accepts_new_tenants (true) }`.

### Generic registry

- `RegistryResource` — `type Key`, `const KIND`, `key()`, `revision()`.
- `ResourceRegistry<T>` — `lookup()`, `apply_all()`, `apply_one()`,
  `invalidate()`, `subscribe()`, `is_primed()`, `len()`.
- Aliases: `TenantRegistry`, `DataSourceRegistry`, `TenantChange`, `DataSourceChange`.
- `LookupError::{Unavailable, NotFound}` — mapped to `ResolveError` by the resolver.
- `ApplyReport { added, updated, removed, stale_ignored, unchanged }`, `is_noop()`.
- `ResourceChange<K> { key, kind, previous_revision, current_revision }`,
  `ChangeKind::{Added, Updated, Removed}`.
- `ResourceSource<T>` (async trait) — `load()`, `describe()`.
  `InMemorySource<T>` (`set`, `fail_next`), `JsonFileSource<T>`.
- `ResourceRefresher::prime()` / `::spawn()`, `RefreshHandle` (`refresh_now`,
  `shutdown`).

### Wiring

- `RuntimeConfig { refresh_interval_seconds (30), fail_fast_on_prime (true) }`.
- `build_runtime(&config, tenant_source, data_source_source)
  -> Result<(Arc<RuntimeResolver>, RuntimeHandles), String>`.
  **Primes DataSources first**, then tenants.
- `RuntimeHandles { tenants, data_sources }`, `shutdown()`.

### Errors

- `ResolveError::{RuntimeUnavailable, UnknownTenant, UnboundDataSource{tenant, logical},
  MissingDataSource{logical, data_source}}`.
- `SourceError::{Unreadable{origin, cause}, Malformed{origin, detail}}` — field is
  `origin`, **not** `source` (thiserror reserves it).
- `ConfigurationError::{TenantHasNoDataBindings, InvalidDataSource}`.

## Module layout

```
resource/          generic lifecycle
  resource_kind.rs registry.rs (+ apply_all, apply_one, tests)
  snapshot.rs change.rs apply_report.rs lookup_error.rs
  source.rs refresher.rs (+ refresh_handle) sources/{in_memory,json_file}
tenant/            tenant_runtime_binding, tenant_data_binding,
                   configuration_binding, storage_binding
data_source/       data_source_resource, placement_class, residency,
                   pool_settings, capabilities
resolution/        runtime_resolver, resolved_data_source
config.rs errors/ logging.rs registration.rs testing.rs
```

## Hard invariants — do not break

1. **Unprimed ≠ empty.** `RuntimeUnavailable` (503) must stay distinguishable
   from `UnknownTenant`.
2. **A load failure never touches a registry.** Never convert a read error into
   `Ok(vec![])`.
3. **Revisions only move forward.** Never apply an older revision.
4. **Tenant bindings carry no physical configuration.** `deny_unknown_fields`
   enforces it; an example test asserts the shipped file mentions no connector,
   pool, or endpoint.
5. **`RuntimeResolver` is the only way to build an `ExecutionTarget`.**
6. **A missing DataSource fails closed.** Never fall back to another.
7. **No control-plane access from a `ResourceSource`.** No Git, no Kubernetes.
8. **No fallback on resolution failure.** No default tenant, no first-available
   database.
9. **Publish change events after the snapshot swap.**
10. Physical detail never escapes upward except as an `ExecutionTarget` (which
    goes *down* to a connector) or a telemetry label.
