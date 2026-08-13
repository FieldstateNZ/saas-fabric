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
  capabilities, labels }`. `validate()`, `telemetry_label()`. `validate()` is
  deliberately `Ok(())`: nothing on a DataSource *alone* can stop this process
  turning it into an `ExecutionTarget`, and refusing one is far more damaging
  than any fault it could catch (see below).
- `PlacementClass` — Shared, Dedicated, HighAvailability, Regulated,
  Development, Ephemeral. `as_str()`, `is_production()`.
- `DataResidency { region, jurisdiction }`. `in_region()`, `telemetry_label()`.
- `PoolSettings { max_connections (20), idle_timeout_seconds (300),
  acquire_timeout_seconds (5) }`. `validate()`. Applied by *reconciliation* to
  the connector, not by this process (ADR 0001) — and, since the same is true
  of the check, `validate()` is reconciliation's to call. Nothing in this
  workspace calls it now; the runtime carries these numbers and never reads
  them.
- `DataSourceCapabilities { writable (true), accepts_new_tenants (true) }`.

### Generic registry

- `RegistryResource: Clone + PartialEq + Send + Sync + 'static` — `type Key`,
  `const KIND`, `key()`, `revision()`, `validate()` (no default impl, so a new
  resource type cannot opt out silently). The `PartialEq` bound exists so the
  apply path can detect a same-revision payload mismatch (item 50); both
  concrete types already derived it.
- `ResourceRegistry<T>` — `lookup()`, `apply_all()`, `apply_one()`,
  `invalidate()`, `subscribe()`, `is_primed()`, `len()`.
- Aliases: `TenantRegistry`, `DataSourceRegistry`, `TenantChange`, `DataSourceChange`.
- `LookupError::{Unavailable, NotFound}` — mapped to `ResolveError` by the resolver.
- `ApplyReport { added, updated, removed, stale_ignored, unchanged,
  invalid_rejected, divergent_payload, duplicate_rejected }`, `is_noop()`.
  `divergent_payload` counts (and a warn log records) an incoming resource at
  the same revision as one held but with a different payload — rejected, never
  applied; the revision is the authority. `duplicate_rejected` counts (and an
  error log records) a key appearing twice in *one* incoming set: the first
  entry decides the key by position, later ones are refused. Only `added`,
  `updated` and `removed` count as movement for `is_noop()`.
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
- `SourceError::{Unreadable{origin, cause}, Malformed{origin, detail},
  NothingUsable{origin, count, reason}}` — field is `origin`, **not** `source`
  (thiserror reserves it). `NothingUsable` is produced only by `prime`, never by
  a refresh.
- `ConfigurationError::{TenantHasNoDataBindings, InvalidResource}`.

## Module layout

```
resource/          generic lifecycle
  resource_kind.rs registry.rs (+ apply_all, apply_one, tests)
  snapshot.rs change.rs apply_report.rs lookup_error.rs
  source.rs refresher.rs (+ refresh_handle, prime_guard) sources/{in_memory,json_file}
  registry/{apply,change,lookup,stale_revision,deletion,concurrency}_tests.rs
  registry/{validation,duplicate_key,writer_concurrency}_tests.rs
  registry/{merge,write_lock}.rs registry/test_resource.rs
tenant/            tenant_runtime_binding, tenant_data_binding,
                   configuration_binding, storage_binding
data_source/       data_source_resource, placement_class, residency,
                   pool_settings, capabilities
resolution/        runtime_resolver, resolved_data_source
registration/      registration.rs (+ registration_tests.rs)
config.rs errors/ logging.rs testing.rs
```

## Hard invariants — do not break

1. **Unprimed ≠ empty.** `RuntimeUnavailable` (503) must stay distinguishable
   from `UnknownTenant`.
2. **A load failure never touches a registry, and never becomes an empty set.**
   Never convert a read error into `Ok(vec![])`. Always logged at error level —
   never silent. On a *first* load this extends to validation: a non-empty
   source whose every entry is rejected has produced nothing to install, so
   `prime` refuses it (`SourceError::NothingUsable`) *before* applying, leaving
   the registry unprimed. Primed-and-empty would answer `/ready` 200 and then
   500 every request. A *partial* rejection is not a failure — there is
   something to serve, and failing would take healthy tenants down over one
   operator's typo.
3. **Revisions only move forward — within one apply as well as across applies.**
   Never apply an older revision. Never apply a *matching* revision whose
   payload disagrees with what is held either (item 50) — count it in
   `ApplyReport::divergent_payload` and warn-log it instead.
4. **Every published event describes a transition into the snapshot that was
   actually installed.** Each incoming entry is judged against the snapshot
   being *replaced*, so a key may be decided only once per `apply_all`; a
   repeated key is refused (`ApplyReport::duplicate_rejected`) rather than
   compared a second time. Without this a losing duplicate publishes an event
   for a snapshot that never existed, and a subscriber acting on it (§19) looks
   up a revision the registry does not hold.
5. **Tenant bindings carry no physical configuration.** `deny_unknown_fields`
   enforces it; an example test asserts the shipped file mentions no connector,
   pool, or endpoint.
6. **`RuntimeResolver` is the only way to build an `ExecutionTarget`.**
7. **A missing DataSource fails closed.** Never fall back to another — including
   a DataSource that existed and was later removed while tenants stayed bound
   to it.
8. **No control-plane access from a `ResourceSource`.** No Git, no live
   per-`load()` Kubernetes API call. `ResourceSource<T>` itself is fully
   generic (`load()` + `describe()`); `JsonFileSource` is one adapter, not an
   assumption baked into anything above `sources/`.
9. **No fallback on resolution failure.** No default tenant, no first-available
   database.
10. **Publish change events after the snapshot swap.**
11. Physical detail never escapes upward except as an `ExecutionTarget` (which
    goes *down* to a connector) or a telemetry label.
12. **DataSources prime before tenant bindings** (`registration.rs`), and a
    snapshot swap is atomic — one reader never sees a half-applied mix of two
    generations. See docs/README.md "Startup consistency model".
