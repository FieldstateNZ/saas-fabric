# fabric-tenant-runtime

What each tenant **currently has**.

## The model

```
tenant → logical binding (primary) → DataSource → connector → infrastructure
```

Two reconciled resources, deliberately separate. See
[ADR 0003](../../../docs/decisions/0003-data-sources-are-first-class-resources.md).

### A tenant binding answers one question

*Which DataSource is this logical name bound to, and how is this tenant isolated
within it?*

```json
{
  "tenant": "acme",
  "revision": 42,
  "data": {
    "primary": { "data_source": "sql-au-east-03", "isolation": { "kind": "database" } }
  }
}
```

Two fields, both genuinely per tenant. Isolation lives here because a schema
name or discriminator value is meaningless outside the context of one tenant.

### A DataSource owns every physical concern

```json
{
  "id": "sql-au-east-03",
  "revision": 4,
  "connector": "postgres-au-east",
  "connection": { "kind": "named", "name": "acme-prod" },
  "placement": "dedicated",
  "residency": { "region": "au-east", "jurisdiction": "AU" },
  "pool": { "max_connections": 20 },
  "capabilities": { "writable": true, "accepts_new_tenants": true },
  "labels": { "owner": "platform" }
}
```

### Why they are separate

A DataSource is shared. Two hundred tenants reference `shared-postgres-02`; they
do not each carry a copy of its endpoint.

- **One edit, one revision.** Correcting an endpoint changes one resource and
  bumps one revision, instead of rewriting two hundred tenant records and
  invalidating all their cached bindings.
- **It can be observed.** "What is the state of `sql-au-east-03`?" has an answer.
- **It can be drained.** `accepts_new_tenants: false` stops new placement while
  existing tenants keep working.
- **They reconcile independently.** Provisioning a DataSource and binding a
  tenant to it are separate acts — which is what a staged migration needs (§19).

`RuntimeResolver` is the only supported way to walk the chain.

## Desired state is not runtime state

Git holds what a tenant *should* have; this crate holds what it *does* have.
§6 insists only the second appears in request handling.

```
Git desired state → reconciliation → runtime registries → requests
```

The shape §6 forbids is a request reaching sideways for its answer: read tenant
→ query Git → query Kubernetes → discover database → resolve secret → execute.
Every step is a network call on the request path, and each turns a control-plane
outage into a data-plane outage.

So reconciliation writes ahead of time, the registries hold the result in
memory, and resolution is an atomic pointer load plus two hash lookups.

## The failure modes, and why they differ

| `ResolveError` | Meaning | Status |
|---|---|---|
| `RuntimeUnavailable` | A registry has not loaded yet | 503 |
| `UnknownTenant` | Snapshot loaded, tenant not in it | 403 |
| `UnboundDataSource` | Tenant declared no such logical name | 500 |
| `MissingDataSource` | Binding points at a DataSource that does not exist | 500 |

**A fresh registry holds no snapshot at all — not an empty one.** That is why
the first two are distinguishable. During a cold start, "unknown tenant" would
tell every caller their tenant had been deleted.

The last two are reconciliation gaps, not caller errors. Both fail closed; there
is no fallback to another logical name or another DataSource.

## The generic registry

Both resources share one implementation (`resource/`): revisioned snapshots,
`ArcSwapOption` lookup, revision-guarded apply, invalidation, change broadcast,
and a polling refresher. Writing it twice would mean two chances to get the
revision guard subtly wrong, and only one of them would have the tests.

A type joins by implementing `RegistryResource` — a key, a revision, and a
`KIND` string for logs.

### Snapshot swapping

Resolution runs on every request and must not contend. An `RwLock<HashMap>`
would serialise readers against a refresh, and a refresh touches every entry, so
the whole request path would stall for its duration.

Instead the registry builds a fresh snapshot to one side and swaps a pointer.
Readers never block; an in-flight request keeps the snapshot it started with
rather than seeing a half-applied update.

### Revisions

Every resource carries one, and they only move forward (§20). That buys
stale-update rejection, migration cut-over ("publish revision N+1"), and a
diagnostic answer to "which revision served this trace?".

The two revisions are independent: resizing a pool bumps the DataSource's and no
tenant's.

### Same revision, different payload

An incoming resource can arrive at *exactly* the revision already held but
carrying different content — almost always a reconciler bug: a real change
that forgot to bump the revision. `apply_all` and `apply_one` both refuse to
apply it (the revision, not the payload, is the authority on whether
something changed), but they no longer swallow it silently into "unchanged".
It is counted in `ApplyReport::divergent_payload` and warn-logged by key,
kind, and revision, so a reconciler that ships a change without bumping the
revision leaves a trail instead of vanishing. See the rustdoc on
`ApplyReport` for the full "why reject rather than accept" reasoning.

### The source abstraction is genuinely generic

`ResourceSource<T>` is the whole contract: `load() -> Result<Vec<T>, SourceError>`
plus a `describe()` for logs. `JsonFileSource` is one adapter, not a
foundation the rest of the runtime assumes. Nothing above the `sources/`
directory reasons about paths, mounts, or file I/O — `ResourceRefresher`,
`ResourceRegistry`, and `RuntimeResolver` only ever see the trait.

Other adapters that would slot in without redesigning anything above
`sources/`:

- A Kubernetes watch over a CRD — **with a caveat**. `load()` must read an
  already-synced local informer cache and return promptly from memory, never
  make a live call to the API server per invocation. That is what keeps it
  from becoming the control-plane dependency `ResourceSource`'s own docs
  forbid (see the gotcha below): a warm local cache goes stale if the
  connection to the API server drops, exactly like a stale file mount, and
  staleness is a case this crate already handles everywhere. A synchronous
  per-`load()` API request would not be — that is indistinguishable from the
  Git-in-the-request-path shape §6 rules out.
- A gRPC streaming client, reconnecting on failure and returning `Err` rather
  than a partial set while disconnected.
- A Unix domain socket to a local reconciler sidecar.
- A client for a shared internal configuration service (an HTTP API in front
  of the same reconciled state a file would otherwise hold) — acceptable on
  the same terms as the Kubernetes case: it must not become something the
  data plane cannot survive without.
- A memory-mapped snapshot another process writes, for the lowest possible
  read latency on the same host.

Each only has to implement `load()` and `describe()` and uphold the one rule
in `ResourceSource`'s own docs: never turn a read failure into `Ok(vec![])`,
and never make freshness depend on a system that is allowed to be down while
tenants keep working.

## Refresh: poll *and* trigger

`RefreshHandle::refresh_now()` is the fast path — a reconciler that just changed
something says so and it lands in milliseconds.

The interval poll is the safety net. Notifications get lost: a pod restarts
mid-flight, a webhook 500s, a partition eats it. Without the poll, one lost
notification strands a resource on stale state forever and nothing notices.

## Startup consistency model

DataSources and tenant bindings are reconciled **independently** — different
sources, different schedules, different revisions. There is no cross-resource
transaction. That means the runtime can, at any moment, hold DataSources at
revision A and tenant bindings whose references were computed against a
different, later or earlier, view of the DataSource fleet. This is not a bug
to eliminate; it is the shape of having two independently reconciled
resources at all (§6/ADR 0003), and every other guarantee in this section
exists to make that shape safe rather than to make it disappear.

**DataSources prime before tenant bindings.** `build_runtime`
(`registration.rs`) loads the DataSource registry to completion before it
loads the tenant registry — see `prime` being called for
`data_source_source` first, `tenant_source` second. This is a real ordering,
not a race resolved by luck: `TenantRegistry` cannot begin priming until
`DataSourceRegistry`'s `load().await` has returned. The ordering guarantee is
pinned by a test (`registration/registration_tests.rs`) that swaps in a
source recording when each was asked to load, and asserts DataSources came
first.

Why this order and not the reverse: a tenant binding is close to useless
without its DataSource resolvable, but a DataSource is perfectly well-formed
on its own (nothing tenant-related references it yet). Priming DataSources
first means that by the time the first tenant binding is visible, the
DataSource it names has the best chance of already being loaded — narrowing,
though not eliminating, the startup window where a resolve would otherwise
fail.

**Dangling bindings fail closed, always.** Even with the priming order
above, a tenant binding can still reference a DataSource the registry does
not currently hold — a DataSource genuinely removed while tenants remain
bound to it, reconciliation racing across the two resources' own refresh
intervals after startup, or (despite the ordering) a very early request
landing between the two primes. Every one of these resolves to
`ResolveError::MissingDataSource`. There is no fallback to a different
DataSource and no default — see `resolution/runtime_resolver_tests.rs` for
the case where a DataSource that genuinely existed is later removed while a
tenant is still bound to it.

**A failed refresh never clears a registry, and never does so quietly.**
`ResourceSource::load` returning `Err` leaves the registry's current snapshot
exactly as it was (`resource/refresher.rs`); only a successful load is ever
applied. This is not silent tolerance of staleness — `logging::refresh_failed`
logs at **error** level, naming the resource kind and the reason, and says
explicitly that the last good snapshot is still serving. An operator reading
logs always has a way to know the runtime is running on stale state; the
runtime never manufactures a "closed" state out of an I/O error by emptying
itself.

**A transient inconsistent pairing cannot corrupt either registry.** The
DataSource-A / tenant-B mismatch described above is a statement about the
relationship *between* two registries, never about the internal state of
either one. Each registry's own snapshot is always a complete, internally
consistent whole — built off to one side and installed with a single atomic
pointer store, so a reader never observes a half-applied mix of an old and a
new snapshot (proven under real concurrency in
`resource/registry/concurrency_tests.rs`, item 48). Cross-resource staleness
can only ever produce one of two outcomes at the resolution seam: a
successful resolve using whatever each registry currently and coherently
holds, or a clean `MissingDataSource`/`UnboundDataSource` rejection. Neither
outcome touches, corrupts, or partially updates a registry.

## Getting started

```rust,ignore
let (runtime, refresh) = build_runtime(
    &config,
    Arc::new(JsonFileSource::new("/etc/fabric/tenants.json")),
    Arc::new(JsonFileSource::new("/etc/fabric/data-sources.json")),
).await?;

// On the request path:
let resolved = runtime.resolve_data_source(identity.tenant(), &primary)?;
if operation.is_write() && !resolved.is_writable() { /* refuse */ }
```

Hold the `RuntimeHandles` until shutdown — dropping them orphans the background
tasks.

## Gotchas

- **A load failure must never become an empty set.** `Err` leaves the current
  snapshot serving; `Ok(vec![])` removes everything. `JsonFileSource` returns
  `Err` for a missing file precisely for this, and there is a test pinning it.
- **`apply_all` is a full sync.** Absent resources are removed — that is what
  makes deprovisioning work, and it is why the point above matters so much.
- **DataSources are primed before tenants.** A binding referencing a DataSource
  the registry has not loaded resolves to `MissingDataSource`, so this order
  avoids a window of spurious 500s at startup. See "Startup consistency
  model" above.
- **Same revision, different payload is rejected, not silently accepted.**
  `apply_all`/`apply_one` never apply a payload that disagrees with a
  revision already held — even the fresh one. It is counted in
  `ApplyReport::divergent_payload` and warn-logged instead.
- **`LogicalDataSourceName` vs `DataSourceId`.** The first is logical (`primary`), the
  second is a DataSource resource (`sql-au-east-03`). They are different types
  for a reason.
- **A `ResourceSource` must not query the control plane live, per load.** A
  Git client, or a Kubernetes client making an API-server call inside
  `load()`, puts control-plane availability behind data-plane freshness. When
  Git is down, tenants should keep working. This is a constraint on what
  `load()` may *do* on each call, not on what technology may back it — see
  "The source abstraction is genuinely generic" above for the (narrow) shape
  a Kubernetes-backed adapter would have to take to stay on the right side of
  this rule.
- **`writable: false` is enforced before the connector is called** — and is
  distinct from whether the connector supports mutations. Both are checked.
- Log target is `fabric_tenant_runtime` (underscores). Events carry
  `resource_kind` so tenant and DataSource lines are distinguishable.
