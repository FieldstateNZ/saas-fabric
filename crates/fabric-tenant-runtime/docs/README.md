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

## Refresh: poll *and* trigger

`RefreshHandle::refresh_now()` is the fast path — a reconciler that just changed
something says so and it lands in milliseconds.

The interval poll is the safety net. Notifications get lost: a pod restarts
mid-flight, a webhook 500s, a partition eats it. Without the poll, one lost
notification strands a resource on stale state forever and nothing notices.

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
  avoids a window of spurious 500s at startup.
- **`DataSourceName` vs `DataSourceId`.** The first is logical (`primary`), the
  second is a DataSource resource (`sql-au-east-03`). They are different types
  for a reason.
- **A `ResourceSource` must not query the control plane.** A Git client or
  Kubernetes watch here puts control-plane availability behind data-plane
  availability. When Git is down, tenants should keep working.
- **`writable: false` is enforced before the connector is called** — and is
  distinct from whether the connector supports mutations. Both are checked.
- Log target is `fabric_tenant_runtime` (underscores). Events carry
  `resource_kind` so tenant and DataSource lines are distinguishable.
