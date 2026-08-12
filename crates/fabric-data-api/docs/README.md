# fabric-data-api

The abstraction boundary between application business logic and tenant data
infrastructure.

## The contract

```http
POST /v1/data/customers
Authorization: Bearer <token>

{"name": "Alice", "email": "alice@example.com"}
```

The application does not name a tenant. It does not name a database. It holds no
connection string, knows no placement class, and cannot tell which isolation
model it is getting (§2, §26).

## What one request does

```
POST /v1/data/customers
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

## Versioning

The full external path is `/v1/data/...` — this crate builds the whole prefix
(`routes::API_PREFIX`), not just the `/v1` part, so the mapping from a version
to the path segment that carries it lives in one file rather than being split
between this crate and its host.

The policy the prefix exists to enforce:

- **`v1` is stable.** Once shipped, a field does not change type or meaning,
  a status code does not change for the same failure, and a route does not
  disappear.
- **Changes within `v1` are additive only** — a new optional query parameter,
  a new field in a response body, a new resource in the catalogue, a new
  operation on an existing resource. An application written against `v1`
  today keeps working unmodified as the platform adds capability.
- **A breaking change ships as `/v2`, mounted alongside `/v1`, never in place
  of it.** Removing a field, tightening validation so a previously-valid
  request is now rejected, or changing what a status code means are breaking.
  Both versions run simultaneously so existing integrations migrate on their
  own schedule rather than being forced to move the moment `/v2` exists.

## The HTTP surface

```
GET    /v1/data/{resource}          list
POST   /v1/data/{resource}          create
GET    /v1/data/{resource}/{key}    read
PATCH  /v1/data/{resource}/{key}    update
DELETE /v1/data/{resource}/{key}    delete
```

No path segment, query parameter, or header selects a tenant. The same URL means
different data for different callers — that is the point.

### Why this shape, deliberately

This is a small, ordinary REST resource/collection convention
(`/{resource}` and `/{resource}/{key}`), chosen on purpose rather than
inherited from any one connector protocol or existing tool:

- **Not NDC-shaped.** NDC (see the SaaS Fabric NDC boundary note) is this
  platform's internal connector protocol — a query/mutation RPC surface
  designed for a connector to implement, not for an application to call.
  Nothing in this crate's public contract is protocol-shaped: no NDC request
  envelope, no connector capability negotiation, no NDC scalar types. An
  application never learns that NDC exists.
- **Not PostgREST-shaped.** No operator-embedded-in-value filter syntax
  (`col=eq.val`), no embedded-resource expansion via query parameters, no
  `Prefer` header vocabulary. The query language this crate offers (`limit`,
  `offset`, `sort`, `select`, and equality filters) is deliberately smaller —
  see "The query language" below for why.
- **Not DAB-shaped.** No GraphQL surface, no OData filter grammar. A caller
  needing genuinely relational queries — joins, aggregation, arbitrary
  boolean predicates — should get a purpose-built catalogue resource, which
  can be indexed, reviewed, and authorised on its own terms, rather than an
  expressive query language every connector has to support faithfully.
- **`PATCH`, not `PUT`.** The update handler applies only the fields it is
  given and leaves the rest alone — that is a patch. `PUT` conventionally
  means whole-record replacement; offering it here would mean a client that
  omits a field silently nulls it, which is a bad default for callers that
  may be working from a partial view of a record.
- **No `PUT`-as-upsert either.** Create and update are distinct operations
  (`POST` vs `PATCH`) with distinct authorization scopes
  (`data:{resource}:write` governs both, but the catalogue's `operations`
  list can expose one without the other). Collapsing them would remove that
  distinction.

### The query language

```
GET /v1/data/customers?status=active&limit=25&offset=50&sort=-created_at&select=id,name
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

## Pagination

`limit`/`offset` paging, and nothing else: no connector-specific cursor or
pagination token is ever exposed, because a token is a place to hide physical
detail (a primary-key range, an internal row id, a shard boundary) and this
crate's entire job is to not do that (§2).

`has_more` is derived, not guessed: every list query asks the connector for
`limit + 1` rows, and the extra "probe" row is trimmed from `data` before the
response is built (`ListResponse::from_outcome`). If the probe row came back,
there is at least one more row; if it did not, there is not. This costs one
extra row per page and needs no total count, which the connector is never
asked for.

**Stable pagination across pages is the caller's responsibility, not
something this crate enforces.** Paging by `offset` is only stable if the
result order is deterministic across requests, which requires a `sort` that
fully orders the rows — ties on a non-unique sort key can be returned in
either order on different requests, which can shift rows across a page
boundary, and it can shift further if rows are inserted or deleted between
pages.

This is a deliberate choice, not an oversight: this crate cannot verify that
a caller-supplied `sort` is actually unique for a given collection — it has
no notion of which columns a connector's schema treats as a key or a unique
index, and inventing a rule (such as silently forcing `key_field` into every
sort) would both over-constrain legitimate queries that do not need
uniqueness and still not guarantee determinism if `key_field` itself is not
actually unique for that collection. A caller that needs stable paging should
include a field (typically the resource's key) in `sort` that is unique for
the rows being paged.

## Request limits

`max_limit` bounds how much data one response can carry. It does not bound
how much *work* a request can demand before a connector ever sees it — and in
a multi-tenant service, one caller's expensive request is every co-tenant's
latency (§28). `DataApiConfig` carries six more bounds for that, all
validated at startup (`DataApiConfig::validate`) and all enforced before any
connector call:

| Limit | Config field | Default | Enforced |
|---|---|---|---|
| Equality filters per list request | `max_filters` | 25 | `DataApiService::list`, via `limits::enforce_query` |
| `sort` fields per list request | `max_sort_fields` | 5 | same |
| `select` fields per list request | `max_select_fields` | 50 | same |
| Filter tree nesting depth | `max_filter_depth` | 4 | same |
| Request body size, in bytes | `max_request_body_bytes` | 1,048,576 (1 MiB) | `extraction::BoundedJson`, while the body is being read |
| Rows in one `POST` | `max_mutation_batch_size` | 500 | `DataApiService::create`, via `limits::enforce_batch_size` |

Every one of these answers a violation with `400 bad_request`, naming the
count and the boundary — never anything physical. The body-size limit is
enforced by capping how many bytes `axum::body::to_bytes` will read, not by
trusting a caller-supplied `Content-Length`: a limit that only checks the
header is not a limit, since nothing stops a client from lying about it or
streaming past it.

The filter-depth limit is the one bound with nothing to trigger it today: the
query language this crate parses only ever builds an `And` of equality
comparisons, which is depth two at most. It is enforced anyway, against the
general shape of a `Filter` tree, so that if the query language ever grows a
nested predicate, there is already a ceiling in place rather than one added
after the regression is found.

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
| A request limit is exceeded | 400 | see "Request limits" |
| Connector rejected | 500 + generic message | its text names tables and servers |

### The public error body

Every error is `{"error": {"code", "message", "request_id"}}`. `code` is a
small, hand-maintained, stable vocabulary (`unknown_tenant`, `read_only`,
`forbidden`, `bad_request`, `internal`, …) — never a serialised Rust enum
variant name, and never renamed just because an internal type is refactored.
A client is expected to branch on `code`, not on `message` text.

`request_id` is always present, even on a 400 the caller already understands,
because it costs nothing and is never sensitive: it is either the caller's
own `X-Request-Id` header echoed back, or a fresh id generated for them (see
"Correlation ids" below).

### Unknown tenant is an anti-enumeration measure

An authenticated caller naming a tenant this runtime has no binding for gets
exactly the same 403 `unknown_tenant` response whether that tenant never
existed, was deprovisioned, or (were such a state ever added) disabled —
`fabric-tenant-runtime`'s `ResolveError::UnknownTenant` deliberately does not
distinguish those cases, and this crate has no way to ask it to. Answering
404 here, or varying the message, would let a caller learn which tenant ids
are live by watching status codes. There is currently no "tenant disabled"
concept anywhere in this platform's error types — nothing here fakes one by
overloading `UnknownTenant`, and if a genuine disabled-tenant state is ever
introduced, it should collapse into this same externally-generic 403 rather
than adding a distinguishable response.

Externally identical does not mean internally invisible: a probe against an
unknown tenant is its own tracing event (`data_api.unknown_tenant_probed`,
naming the tenant), separate from the generic `data_api.request_failed` event
platform-side (500/503) failures produce. An operator watching logs can tell
"someone is probing tenant ids" apart from "the runtime has a reconciliation
gap" — a caller watching only response codes and bodies cannot tell either
apart from an ordinary, honest "no such tenant" answer.

### Correlation ids

Every response — success or failure — carries an `X-Request-Id` header. If
the caller sent one, it is echoed back unchanged, so a gateway's or a
caller's own tracing shares one id with this crate's logs; otherwise a fresh
id is generated. The same id appears in three places for a failure: the
response body (`error.request_id`), the response header, and the `tracing`
event that recorded whatever detail the response withheld
(`data_api.request_failed` for a masked 5xx, `data_api.unknown_tenant_probed`
for a probed tenant). Quoting the id from a client error report is enough to
find the matching internal log line — nothing else about the failure needs
to be guessed or reproduced.

## Read-only DataSources

`OperationNotAllowed` (405) says the *catalogue* does not expose this verb.
`ResourceIsReadOnly` (405) says this *tenant's placement* does not — the same
catalogue entry is writable for a tenant on a primary and read-only for one on a
replica.

The check happens in `prepare`, before the connector is called, and it is
distinct from whether the connector supports mutations. Both are checked, and
either saying no is a no (§28). The message says only "read-only": which
DataSource, and why, stays internal.

## Cancellation

When a client disconnects, axum drops the handler future at its next await
point. Nothing in this crate defeats that: no handler, service method, or
connector call is `tokio::spawn`ed onto a detached task, so a dropped request
genuinely stops the in-flight connector call rather than letting it run to
completion in the background. This is exercised directly in
`tests/cancellation.rs`, using a connector that sleeps and records whether it
was ever polled to completion — the test proves a dropped request's connector
call never finishes, not merely that the HTTP response never arrived.

This has a real limit, and it is a limit on cancellation in general, not
something specific to this crate: **a mutation already sent to the connector
cannot be un-sent.** Cancellation stops *this crate* from continuing to await
the connector's future; it says nothing about what the connector's own
transport has already written to the wire, or what the backend has already
committed, by the time that future is dropped. Whether a cancelled `create`,
`update`, or `delete` partially or fully took effect depends on the
connector's own transactional behaviour, which this crate does not control
and cannot observe once the future is gone. A caller that disconnects mid
write should not assume the write did not happen.

## Gotchas

- **Connector error text never reaches a caller.** It names physical tables,
  schemas, and servers. It is logged with the request id and replaced with
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
- **`limit` is clamped, not rejected; every other request limit is rejected,
  not clamped.** There is no sensible way to silently drop a caller's filter,
  sort field, projection, or batch row and still answer the request they
  asked for — so those come back as 400 instead.
- **Stable paging needs a unique sort key, and this crate does not enforce
  one.** See "Pagination".
- **The host must mount this crate's router as-is.** `data_routes` already
  builds the full `/v1/data/...` path; nesting it under a further prefix
  produces a doubled path.
- Log target is `fabric_data_api`.
