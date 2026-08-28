# Crate dependency direction

The dependency graph is an invariant, not an accident. It is what keeps the
neutral connector boundary replaceable, keeps HTTP concerns out of the runtime,
and will keep the Experience, Configuration, Feature and Storage APIs from
tangling into each other as they arrive.

## The intended graph

```
fabric-core            (nothing internal)

fabric-identity        → fabric-core

fabric-connector       → fabric-core

fabric-tenant-runtime  → fabric-core
                       → fabric-connector

fabric-connector-ndc   → fabric-core
                       → fabric-connector

fabric-data-api        → fabric-core
                       → fabric-identity
                       → fabric-tenant-runtime
                       → fabric-connector

fabric-api             → all of the above (composition root)
```

Verified as of the current tree; it matches exactly.

## The rules behind it

**`fabric-core` knows nothing about anything.** No Axum, no NDC, no runtime, no
async. It holds validated identifiers, the event-ID scheme and the clock seam.
Every crate depends on it, so everything added there is paid for everywhere —
see "not a dumping ground" below.

**`fabric-connector` knows nothing about HTTP or the Data API.** It is the
neutral execution boundary. A native PostgreSQL provider must be able to
implement it without ever hearing the words "logical resource" or "status code".

**`fabric-tenant-runtime` knows nothing about Axum handlers.** It resolves
tenants to execution targets. It has no idea a request is HTTP-shaped, which is
what will let the Configuration and Storage APIs reuse it unchanged.

**`fabric-identity` does not depend on the Data API.** It derives a tenant
identity context; who consumes it is not its business.

It *does* depend on Axum, and that is deliberate rather than an oversight in
the rule above. Turning an inbound HTTP request into a tenant identity is the
crate's purpose: it owns the extractor that makes `TenantIdentity` a handler
parameter, and the `IntoResponse` for the ways deriving one can fail. The
transport-independent half — the resolver, the token readers, the
configuration — takes an `http::HeaderMap` and knows nothing about a server.
Moving the extractor into `fabric-data-api` would buy a tidier dependency
list at the price of splitting identity extraction across two crates, which
is the worse trade. `fabric-core`, `fabric-connector` and
`fabric-tenant-runtime` remain transport-free, and that is what the
architecture check enforces.

**Only `fabric-api` depends on `fabric-connector-ndc`.** The protocol crate is
wired in at the composition root and nowhere else. If any other crate ever needs
it, NDC has leaked and the boundary has failed.

## Enforcement

Direction is enforced by Cargo: a cycle will not compile, and a new edge
requires editing a `Cargo.toml`, which is visible in review. Beyond that:

- The architecture invariant tests assert that NDC types do not appear outside
  `fabric-connector-ndc`, and that no Git or Kubernetes client exists in any
  request-handling crate.
- CI runs `cargo deny check` including a `[bans]` section, so a surprise
  transitive dependency is visible.

## `fabric-core` is not a dumping ground

The pressure on a shared-kernel crate is always the same: two crates need a
type, so it goes in core, and over time core becomes a bag of every enum in the
system.

The test for admission is **not** "more than one crate uses it". It is:

1. Is it a genuinely cross-cutting domain primitive?
2. Would putting it in the crate that owns the concept create a bad dependency?
3. Is it free of I/O, async and framework types?

If a type conceptually belongs to tenant runtime or connector execution, it
stays there even if two crates import it. `IsolationModel` lives in
`fabric-connector` and not in core, despite being used by the runtime, because
it is an execution concept. `DataSource` lives in `fabric-tenant-runtime` and not
in core, despite being referenced conceptually across the platform, because it
is reconciled runtime state.

What core currently holds, and why each earns its place:

| Type | Why it is cross-cutting |
|---|---|
| `TenantId` | Every crate handles it; validation must be identical everywhere |
| `LogicalResourceName`, `LogicalDataSourceName`, `DataSourceId` | The shared vocabulary of the resolution chain |
| `BindingRevision` | The revision semantics must be one implementation |
| `event_id` / `EventType` | One event-ID scheme across all domains |
| `Clock` | The time seam, so nothing sleeps in tests |
| `naming` | The identifier character rules, so newtypes elsewhere cannot drift |

## Adding a crate

New platform APIs (Configuration, Feature, Storage, Events, Experience) should
sit at the same level as `fabric-data-api`: depending on core, identity, tenant
runtime and connector, depended on only by `fabric-api`. They must not depend on
each other. If two of them need to share something, that is a signal for a new
shared crate or a core primitive — decided deliberately, not by adding an edge.
