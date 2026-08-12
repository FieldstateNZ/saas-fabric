# fabric-data-api

The abstraction boundary between application business logic and tenant data
infrastructure.

## The contract

```http
POST /data/customers
Authorization: Bearer <token>

{"name": "Alice", "email": "alice@example.com"}
```

The application does not name a tenant. It does not name a database. It holds no
connection string, knows no placement class, and cannot tell which isolation
model it is getting (§2, §26).

## What one request does

```
POST /data/customers
  → tenant_id from the bearer token          (fabric-identity, §10)
  → catalogue: customers → logical data source + collection   (§15)
  → tenant binding: logical name → DataSource (fabric-tenant-runtime)
  → DataSource: connector + connection        (ADR 0003)
  → connector executes                        (fabric-connector)
```

The middle two steps belong to `RuntimeResolver`, which this crate holds rather
than reaching into registries itself. That keeps the tenant → DataSource walk in
one place with one set of error mappings, and leaves this crate responsible for
the two ends: what a logical resource means, and what a caller may do with it.

Every arrow is an in-memory lookup. Nothing reads Git, queries Kubernetes, or
opens a connection (§6).

## The HTTP surface

```
GET    /data/{resource}          list
POST   /data/{resource}          create
GET    /data/{resource}/{key}    read
PATCH  /data/{resource}/{key}    update
DELETE /data/{resource}/{key}    delete
```

No path segment, query parameter, or header selects a tenant. The same URL means
different data for different callers — that is the point.

### The query language

```
GET /data/customers?status=active&limit=25&offset=50&sort=-created_at&select=id,name
```

Four reserved parameters — `limit`, `offset`, `sort`, `select`. **Every other
parameter is an equality filter** on the field of that name.

Deliberately modest. A richer language would have to be expressible by every
connector the platform ever talks to, and where it is not, the choice becomes
"refuse common queries" or "translate them unfaithfully". Equality, ordering,
projection, and paging are what every backend does exactly.

Callers needing more should get a purpose-built catalogue resource — which is a
better answer anyway, since it can be indexed, reviewed, and authorised on its
own terms.

## Tenant resolution vs authorization

§23 keeps these apart, and so does this crate:

- **Tenant resolution** — which tenant's resources does this target?
- **Authorization** — may this identity do this?

They meet nowhere. `ResourcePermissions::permits` takes an operation and an
identity and returns a `bool`. It is never handed anything that could change the
tenant and has no way to return one. An administrator is an administrator *of
their own tenant*.

## Failing closed

| Situation | Response | Why |
|---|---|---|
| No token / no tenant claim | 401 | (§28) |
| `X-Tenant-Id` present | 400 | (§11) — rejected, not ignored |
| Runtime not primed | 503 | retryable; **not** "unknown tenant" |
| Unknown tenant | 403 | 404 would let callers enumerate tenants |
| Tenant declared no such logical source | 500 | reconciliation gap; message is "internal error" |
| Binding names a missing DataSource | 500 | reconciliation gap; the id never reaches the caller |
| DataSource is read-only | 405 | placement, not catalogue — see below |
| Uncatalogued resource | 404 | true regardless of tenant, so leaks nothing |
| Operation not in catalogue | 405 | |
| Scope refused | 403 | |
| Connector rejected | 500 + generic message | its text names tables and servers |

## Read-only DataSources

`OperationNotAllowed` (405) says the *catalogue* does not expose this verb.
`ResourceIsReadOnly` (405) says this *tenant's placement* does not — the same
catalogue entry is writable for a tenant on a primary and read-only for one on a
replica.

The check happens in `prepare`, before the connector is called, and it is
distinct from whether the connector supports mutations. Both are checked, and
either saying no is a no (§28). The message says only "read-only": which
DataSource, and why, stays internal.

## Gotchas

- **Connector error text never reaches a caller.** It names physical tables,
  schemas, and servers. It is logged with the trace id and replaced with
  `"internal error"`. There is a test pinning this.
- **A read of another tenant's key is 404, not 403.** 403 would confirm the key
  exists somewhere.
- **A delete of another tenant's key reports 0 affected, not an error.** Same
  reasoning.
- **There is no unfiltered delete.** The route requires a key, so a caller
  cannot empty a collection whatever their scopes.
- **`queryable_fields` covers filters, not just projections.** Filtering is an
  information channel: narrow a filter until rows disappear and you have read a
  hidden value.
- **Resources are read-only by default.** A catalogue entry must deliberately
  list `create`/`update`/`delete`.
- **Paging asks the connector for `limit + 1` rows.** The probe row makes
  `has_more` a fact; it is trimmed before the response is built.
- **`limit` is clamped, not rejected.** One tenant's unbounded scan is every
  co-tenant's latency.
- Log target is `fabric_data_api`.
