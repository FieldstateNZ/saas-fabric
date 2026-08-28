# ADR 0003 — DataSources are first-class resources

- **Status:** Accepted
- **Date:** 2026-08-12
- **Applies to:** `fabric-tenant-runtime`, and the shape of reconciled state
- **Related:** [Platform specification](../architecture/tenant-runtime-data-api.md) §7, §16, §17, §19, §22; [ADR 0001](0001-ndc-as-connector-boundary.md)

## Context

The initial implementation put physical configuration directly inside each
tenant's runtime binding:

```json
{
  "tenant": "acme",
  "data": {
    "primary": {
      "connector": "postgres-au-east",
      "connection": { "kind": "named", "name": "acme-prod" },
      "isolation": { "kind": "database" }
    }
  }
}
```

That works, and for one tenant it looks perfectly reasonable. It stops looking
reasonable at two hundred.

A shared database is referenced by every tenant on it, so its connector,
connection, region, and pool sizing were duplicated across every one of those
tenant records. The consequences compound:

- **Correcting an endpoint means rewriting every tenant on it**, and bumping
  every one of their revisions — so a change that has nothing to do with any
  tenant invalidates all their cached bindings at once.
- **There is no answer to "what is the state of `shared-postgres-02`?"** other
  than reconstructing it from whichever tenants happen to mention it. It cannot
  be listed, monitored, or drained as a thing in its own right.
- **Pool sizing has nowhere to live.** §22's objective is a statement about a
  *database* — "connection count must not scale with replicas × tenants" — and
  there was no object representing the database to attach it to.
- **Nothing could be reconciled independently.** Provisioning a new database and
  binding a tenant to it were the same edit, so they could not be staged, and a
  migration's "provision the target" step had no representation until the
  cut-over happened.

## Decision

Introduce **DataSource** as a first-class reconciled resource, and reduce the
tenant binding to what is genuinely tenant-specific.

```text
tenant → logical binding (primary) → DataSource → connector → infrastructure
```

### A tenant binding answers one question

*Which DataSource is this logical name bound to, and how is this tenant isolated
within it?*

```json
{
  "tenant": "acme",
  "revision": 42,
  "data": {
    "primary": {
      "data_source": "sql-au-east-03",
      "isolation": { "kind": "database" }
    }
  }
}
```

Two fields, and both are per tenant. Isolation stays here because a schema name
or discriminator value is meaningless outside the context of one tenant — it
cannot live on the shared resource.

### A DataSource owns every physical concern

```json
{
  "id": "sql-au-east-03",
  "revision": 4,
  "connector": "postgres-au-east",
  "connection": { "kind": "named", "name": "acme-prod" },
  "placement": "dedicated",
  "residency": { "region": "au-east", "jurisdiction": "AU" },
  "pool": { "max_connections": 20, "idle_timeout_seconds": 300, "acquire_timeout_seconds": 5 },
  "capabilities": { "writable": true, "accepts_new_tenants": true },
  "labels": { "owner": "platform", "tier": "gold" }
}
```

Reconciled from its own source, on its own schedule, with its own revision.

### The two are resolved together, in one place

`RuntimeResolver` owns the chain and is the only supported way to obtain an
`ExecutionTarget`. Neither half is sufficient alone: the DataSource supplies the
connector and connection, the tenant binding supplies the isolation.

## What each field is for

| Field | Why it is on the DataSource |
|---|---|
| `connector`, `connection` | The physical route. Shared by every tenant on it. |
| `pool` | §22 sizing is a property of the database being connected to. Applied by reconciliation to the connector's configuration — see "who acts on this" below. |
| `placement` | §17's service class. Descriptive: reconciliation interprets placement policy when *choosing* a DataSource; by the time a binding exists the choice is made. |
| `residency` | Region and jurisdiction. Contractual, not a performance hint — reconciliation needs somewhere to read it when placing a tenant with an `au-only` requirement. |
| `capabilities` | What the platform permits: `writable` (a read replica) and `accepts_new_tenants` (draining). |
| `labels` | Operator-defined telemetry dimensions, so the type does not grow a field per deployment's taxonomy. |

### `capabilities.writable` is enforced, not decorative

It is checked before a write leaves the process, and it is distinct from
`ConnectorCapabilities`. A read replica's connector can express writes perfectly
well; the replica will reject them at some depth with a vendor-specific error.
Declaring `writable: false` refuses the write with a clear message, no wasted
round trip, and no caller learning which DataSource they are on.

Both checks apply, and either saying no is a no (§28).

### Who acts on `pool`

Not this process. Since [ADR 0001](0001-ndc-as-connector-boundary.md) moved data
execution to connector processes, the pool lives inside the connector, and
reconciliation applies these numbers to its configuration. The settings are
declared here because this is where they belong conceptually — and because a
reviewer asking "does §22 hold?" now has one object to look at per database
rather than a scattering of tenant records.

## Consequences

### Good

- **One edit, one revision.** Correcting an endpoint changes one resource.
  No tenant record is touched and no tenant revision moves.
- **DataSources are observable.** "What is on `shared-postgres-02`, what region
  is it in, is it draining?" has a direct answer.
- **Independent reconciliation.** Provisioning a DataSource and binding a tenant
  to it are separate acts, which is what a staged migration needs (§19).
- **Read-only placement became expressible**, and is enforced fail-closed.
- **Two registries, one lifecycle.** Both resources are revisioned, snapshotted,
  and refreshed by the same generic machinery, so the revision guard has one
  implementation and one set of tests.

### Bad, and accepted

- **A binding can dangle.** A tenant may reference a DataSource the registry
  does not have — a new one not yet propagated, or one removed while tenants
  were still bound. This resolves to `MissingDataSource`, a 500, and fails
  closed. Mitigated by priming DataSources before tenants at startup, and by an
  example test asserting every shipped binding names a DataSource that exists.
- **A second file to reconcile.** Two sources, two refreshers, two failure
  modes. The independence is the point, so this is the cost of the feature
  rather than an oversight.
- **One more hop when reading a binding.** An extra hash lookup per request,
  against an in-memory map.

## Alternatives rejected

| Alternative | Why rejected |
|---|---|
| Keep physical config in the tenant binding | The problem this ADR exists to solve. |
| One file with both arrays | Couples their lifecycles again — the whole file rewrites when either changes, and the two cannot be reconciled on separate schedules. |
| DataSource as connector configuration only | Then it is invisible to the platform: no placement class, no residency, no draining, and no way to refuse a write to a replica before the round trip. |
| Put isolation on the DataSource | A schema name and a discriminator value are per tenant. Putting them on a shared resource is a category error and would make sharing impossible. |

## Invariants this decision must not break

1. Applications never see a `DataSourceId`, a connector, a region, or a
   placement class. All of it stays behind the Data API (§2, §26).
2. Physical detail never appears in an error body — `MissingDataSource` returns
   "internal error".
3. `RuntimeResolver` remains the only way to build an `ExecutionTarget`.
4. Tenant bindings carry no physical configuration. `deny_unknown_fields`
   enforces it at the deserialisation boundary, and an example test asserts the
   shipped tenant file mentions no connector, pool, or endpoint.
5. A missing DataSource fails closed. Never fall back to another one.
