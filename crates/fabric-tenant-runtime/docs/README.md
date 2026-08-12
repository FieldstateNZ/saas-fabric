# fabric-tenant-runtime

What each tenant **currently has**.

## Desired state vs runtime state

Git holds what a tenant *should* have. This crate holds what it *does* have.
Specification §6 insists these are different and that only the second appears in
request handling.

```
Git desired state → reconciliation → Runtime Tenant Registry → requests
```

The shape §6 explicitly forbids is a request reaching sideways for its answer:

```
read tenant → query Git → query Kubernetes → discover database
            → resolve secret → execute
```

Every step there is a network call on the request path, and each one turns a
control-plane outage into a data-plane outage. So reconciliation writes ahead of
time, this registry holds the result in memory, and `resolve()` is an atomic
pointer load plus a hash lookup. No I/O, no locks.

## The three failure modes, and why they differ

`ResolveError` has three variants and the differences matter:

| Variant | Meaning | Status |
|---|---|---|
| `RuntimeUnavailable` | No snapshot loaded yet | 503 |
| `UnknownTenant` | Snapshot loaded, tenant not in it | Rejected |
| `UnknownDataSource` | Tenant exists, didn't declare that data source | Rejected |

**A fresh registry holds no snapshot at all — not an empty one.** That is why
the first two are distinguishable. During a cold start, reporting "unknown
tenant" would tell every caller their tenant had been deleted, and any client
with retry-on-404 disabled would give up rather than wait.

There is no default tenant, no first-available database, no shared fallback.
§28 names all three as things the runtime must never silently do.

## Snapshot swapping

Resolution runs on every request and must not contend. A `RwLock<HashMap>`
would serialise readers against a refresh, and a refresh touches every tenant —
so the whole request path would stall for its duration.

Instead the registry builds a fresh snapshot to one side and swaps a pointer
(`ArcSwapOption`). Readers never block. An in-flight request keeps the snapshot
it started with rather than seeing a half-applied update. The cost is a full
rebuild per refresh — a few thousand `Arc` refcount bumps, cheaper than one
database round trip.

## Revisions

Every binding carries a `BindingRevision`, and revisions only move forward
(§20). That single property buys:

- **Stale-update rejection.** An incoming binding older than the one held is
  ignored. Without this, a stale read could point a tenant back at a database a
  migration has just drained.
- **Migration cut-over.** §19's live migration is "publish revision N+1".
- **Diagnostics.** The revision is stamped on every `ExecutionTarget` and
  emitted in telemetry, so support questions become "which revision served this
  trace?".

## Change propagation

`registry.subscribe()` yields `BindingChange` events. That is the signal for the
layer below to release anything attached to the old binding — a cached target, a
resolved credential, a connector-side connection.

Delivered over a broadcast channel, so a slow subscriber lags rather than
blocking the registry. If you lag, re-read the binding you care about; the
registry always holds current state.

## Refresh: poll *and* trigger

`RefreshHandle::refresh_now()` is the fast path — a reconciler that just changed
a tenant says so and the change lands in milliseconds.

The interval poll is the safety net. Notifications get lost: a pod restarts
mid-flight, a webhook 500s, a partition eats it. Without the poll, one lost
notification strands a tenant on a stale binding forever and nothing notices.

## Getting started

```rust,ignore
let source = Arc::new(FileBindingSource::new("/etc/fabric/bindings.json"));
let (registry, refresh) = build_tenant_runtime(&config, source).await?;

// On the request path:
let binding = registry.resolve(identity.tenant())?;
let target = binding.execution_target(&DataSourceName::try_new("primary")?)?;
```

Hold the `RefreshHandle` until shutdown — dropping it leaves the background task
running with no way to stop it.

## Gotchas

- **A load failure must never become an empty set.** `Err` leaves the current
  snapshot serving; `Ok(vec![])` removes every tenant. `FileBindingSource`
  returns `Err` for a missing file for exactly this reason, and there is a test
  pinning it.
- **`apply_all` is a full sync.** Absent tenants are removed. That is what makes
  deprovisioning work, and it is why the point above matters so much.
- **Staleness is per tenant; removal is not.** An older revision for a tenant
  present in both sets is ignored, but a truncated set still removes.
- **`BindingSource` must not query the control plane.** A Git client or a
  Kubernetes watch here puts control-plane availability behind data-plane
  availability. When Git is down, tenants should keep working.
- Log target is `fabric_tenant_runtime` (underscores).
- A `FileBindingSource` on a mounted `ConfigMap` looks humble but is the right
  shape: no API-server dependency on the request path, staleness bounded by the
  kubelet sync period plus the refresh interval.
