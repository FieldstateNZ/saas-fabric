# Crate dependency direction

The dependency graph is an invariant, not an accident. It is what keeps the
neutral connector boundary replaceable, keeps HTTP concerns out of the runtime,
keeps the control plane out of the request path entirely, and will keep the
Experience, Configuration, Feature and Storage APIs from tangling into each
other as they arrive.

There are **two** graphs, sharing exactly one crate.

## The runtime plane

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

fabric-fga-auth        → fabric-core

fabric-fga-auth-api    → fabric-core
                       → fabric-fga-auth   (composition root, own image)

fabric-api             → all of the above (composition root)
```

**`fabric-fga-auth` depends on `fabric-core` and nothing else in the platform.**
It answers one question — given a token, who is this and which store answers
for them — and it is deliberately reachable from nothing above it yet. It has
no edge to `fabric-identity` even though both read bearer tokens: that one
derives a *tenant* for the Data API from a token the ingress already
established, while this one is the trust boundary itself, verifying signatures
against a registry of issuers. Sharing code between them would mean the thing
that establishes trust and the thing that consumes it could not be reasoned
about separately (ADR 0016).

## The control plane

```
fabric-core            (the only crate both planes share)

fabric-client-model    → fabric-core

fabric-reconciliation  → fabric-core
                       → fabric-client-model

fabric-control-plane   → fabric-core
                       → fabric-client-model
                       → fabric-reconciliation
                       → fabric-platform-management (the platform surface)

fabric-keycloak        → fabric-core
                       → fabric-client-model
                       → fabric-reconciliation      (implements IdentityProvider)
                       → fabric-control-plane       (implements OperatorSignIn)

fabric-git-host        → fabric-core

fabric-platform-git    → fabric-core
                       → fabric-git-host            (the App credential)
                       → fabric-platform-management (the update policy)

fabric-platform-management → fabric-core            (the rules; no transport)

fabric-registry        → fabric-platform-management  (implements Registry)

fabric-client-git      → fabric-core
                       → fabric-client-model
                       → fabric-git-host            (the App credential)
                       → fabric-control-plane       (implements ClientRepository,
                                                     GitAppProvisioning,
                                                     DesiredStateFactory)

fabric-openbao         → fabric-core
                       → fabric-control-plane       (implements SecretStore,
                                                     IntegrationStore)

fabric-control-plane-api → all of the above (composition root)
```

Both verified as of the current tree; they match exactly.

### `fabric-git-host` is shared because the credential is, and the integration is not

SaaS Fabric connects to a Git host for two independent reasons — client desired
state, and platform desired state — and they must be **separate GitHub Apps,
independently installable, configurable and removable**. Two integrations, two
Apps, two repositories, two credentials, and no edge between the adapters.

What is not two things is *how a private key becomes a bearer*: one exchange
with one endpoint, behind a cache whose correctness is subtle enough to have
been got wrong once already — a stated expiry that must be read rather than
assumed, a wall-clock remaining lifetime measured against a monotonic deadline,
and an invalidation path for a token that stops working inside its stated life.
Two copies of that would be two copies of the platform's credential-minting
code, and a fix to one would silently miss the other.

So it depends on `fabric-core` and nothing else, and it knows nothing about
which repository it is authenticating to or what is stored there. It reports
its own `TokenError`; each adapter maps that into its own vocabulary, because
"the credential was refused" leads somewhere different for a client repository
than for a platform one.

`fabric-platform-management` holds the rules and no transport at all. It
defines the `Registry` port and is handed an implementation, because *which*
registry and *how it authenticates* is a separate integration from the platform
repository's credential and the two must stay separable — the GitHub App that
writes desired state is not, and must never become, the registry credential.

`fabric-registry` implements that port and holds no credential at all: the
packages are public, so it exchanges an anonymous pull token and reads. A
credential that does not exist cannot be conflated with the GitHub App that
writes desired state, which is the boundary that matters here.

The edge from `fabric-platform-git` runs the other way and is one type deep:
an update policy is a rule before it is a field in a file, so the rules crate
owns what it *is* and the adapter serialises it. Deciding which version an
environment should run and writing that decision down remain two jobs, and
the composition root is what puts them together.

`fabric-platform-git` is the other side of that: the adapter for the *platform*
repository, with no edge to `fabric-client-git` and none back. Two Apps, two
installations, two repositories, and the only thing in common is the exchange
that turns a key into a bearer.

It has no edge to `fabric-control-plane` yet either, because the port it will
implement does not exist — the Platform Management domain has not been written.
It stands alone until there is something to be the adapter *for*, which is
better than inventing a port to satisfy a shape.

Note where `fabric-git-host` sits relative to the table below: `GitCredential` is listed there
as deliberately *not* shared with the runtime plane's `ResolvedSecret`, and that
stays true. This crate is shared **within** the control plane, between two
adapters on the same side of the boundary.

## The planes do not meet

No crate in one plane may depend on a crate in the other, and
`scripts/check_architecture.py` fails the build if one does.

This is not tidiness. The runtime plane must keep serving tenants while Git and
Keycloak are unreachable, and the control plane must be deployable on a
different network with a different identity model (ADR 0009). One edge between
them puts control-plane availability behind every tenant request, which
specification §6 forbids in as many words.

`fabric-core` is shared deliberately: identifier rules, the event-ID scheme and
the clock seam are genuinely the same concept in both planes, and having two
copies of the tenant-id character set would be worse than the edge.

What is *not* shared, despite looking shareable:

| Runtime | Control plane | Why not shared |
|---|---|---|
| `TenantId` | `ClientId` | Same string, different planes, established by different means. Sharing one type would let a runtime tenant identity reach a control-plane operation. |
| `ResolvedSecret` | `AdminCredential`, `GitCredential` | Forty lines each. Sharing would put a runtime-plane crate in the control plane's graph. |
| `BindingRevision` | `ClientRevision` | A counter you can order, and a content hash you cannot. |
| `telemetry::init` | `telemetry::init` | Fifteen lines, no invariant riding on them being identical. |

## `fabric-runtime-publication` sits in neither plane

`fabric-core` was never the only crate that answered to neither the runtime
plane nor the control plane. `fabric-git-host`, `fabric-platform-git`,
`fabric-platform-management`, and `fabric-registry` all sit there too, and
`fabric-git-host` does network I/O — so "neither plane" has never been the
same claim as "does no I/O." `fabric-runtime-publication` is the sixth crate
in neither plane, and deliberately on the same footing as `fabric-core`
specifically: its only non-dev internal dependency is `fabric-core`.

It owns the *wire contract* for the three documents the runtime already
consumes -- `tenants.json`, `data-sources.json`, `catalog.json` -- plus the
sidecar manifest each one is published beside. It does not, and structurally
cannot, depend on `fabric-tenant-runtime`, `fabric-connector`, or
`fabric-data-api`: those are the runtime-plane crates that own the types this
contract mirrors, and a future control-plane publisher must never gain a path
to them. `fabric-control-plane` therefore still gains no dependency on
`fabric-tenant-runtime` -- the claim the runtime plane's table above already
makes -- and that stays true with this crate in the graph, not despite it.

Because the producer cannot share a Rust type with the consumer without
recreating exactly the edge this document forbids, it declares its own copy
of every wire shape. Fidelity between the two copies is enforced by
`#[serde(deny_unknown_fields)]` on the consumer's own types (a field either
side adds or drops fails loudly) and by round-trip tests that deserialise
this crate's canonical JSON as the consumer's type -- never by a shared
`struct`.

**Its dev-dependencies are wider than its production ones, on purpose.** The
`expected` entry in `scripts/check_architecture.py` declares four dev edges,
each earning its place for a different test:

| Dev-dependency | Why |
|---|---|
| `fabric-tenant-runtime` | the round-trip tests beside `TenantBindingDocument` and `DataSourceDocument` deserialise this crate's canonical JSON as the consumer's own `TenantRuntimeBinding` / `DataSource`, and the composed acceptance test builds the real `fabric_tenant_runtime::build_runtime` over the real `JsonFileSource` |
| `fabric-data-api` | the round-trip test beside `CatalogDocument` deserialises as the consumer's `ResourceCatalog`, and the composed acceptance test builds the real `fabric_data_api::build_data_api` router over it |
| `fabric-identity` | the composed acceptance test mints bearer tokens and builds the real identity extractor stack, so a request reaching the assembled router carries the tenant identity the way a real one would |
| `fabric-connector` | the composed acceptance test's recording connector (`tests/support/connector.rs`) implements the real `DataConnector` trait and captures the real `Filter`/`QuerySpec` the platform builds, so the isolation assertions are checked against the actual predicate rather than a stand-in |

`fabric-tenant-runtime` and `fabric-data-api` arrived with this crate itself,
for the round-trip tests; `fabric-identity` and `fabric-connector` arrived
with the composed acceptance test
(`tests/published_state_serves_two_tenants.rs`), added to `expected` at the
same time the dependency was added — never pre-authorised ahead of a test
that needed it. The check is subset-based, so an edge declared here that the
crate does not actually have would pass silently; keeping the table to
exactly the real edges is what keeps it worth reading. Dev edges are safe
*because* they are dev-only: a `[dev-dependencies]` edge cannot reach the
binary any production caller links, only the test binaries this crate builds
for itself.

That last sentence is doing real work, and it is worth being honest about its
limit: `Graph.direct_dependencies` in `scripts/check_architecture.py` reads
every dependency table, dev included, so `check_dependency_direction` treats
this crate's dev edges to two runtime-plane crates exactly like production
edges for the purpose of the table above. `check_the_planes_do_not_meet`'s
complementary claim -- that no runtime-plane crate can reach *this* crate,
over any table -- is not that check's job either, because it skips any crate
in neither plane outright. ADR 0018 closes both gaps with two checks of their
own, computed straight from the plane sets rather than from `expected`:
`check_plane_reachability_is_transitive` catches a *production* dependency
chain that bridges the planes through a crate in neither one, and
`check_runtime_plane_cannot_reach_the_publisher` refuses this crate
specifically to every runtime-plane crate, dev tables included.

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

**Adapters depend inward; the domain never depends on an adapter.**
`fabric-keycloak` depends on `fabric-reconciliation` and `fabric-control-plane`
for the two ports it implements, and `fabric-client-git` on
`fabric-control-plane` for the same reason.

That Keycloak implements *two* ports is not a smell, it is the honest shape:
Keycloak is two different things to this platform. It is the identity provider
reconciliation drives towards a client's desired state, and it is separately
the realm the platform's own operators authenticate against. Those are
different jobs for different callers, so they are different ports — and a
single `Keycloak` interface covering both would have coupled operator sign-in
to client reconciliation for no reason other than the vendor being the same. The
arrows point that way and not the other, which is what lets the reconciler be
tested against a fake provider and the control plane against an in-memory
repository — with no conditional compilation and no test-only feature flag.

Only `fabric-control-plane-api` depends on any adapter. If any other crate
ever needs one, Keycloak, Git or OpenBao has leaked and the boundary has failed — the
same statement ADR 0001 makes about NDC, for the same reason (ADR 0008).

## Enforcement

Direction is enforced by Cargo: a cycle will not compile, and a new edge
requires editing a `Cargo.toml`, which is visible in review. Beyond that:

- `scripts/check_architecture.py` asserts that NDC types do not appear outside
  `fabric-connector-ndc`, that Keycloak representations stay inside
  `fabric-keycloak` and Git-hosting details inside `fabric-client-git`, that no
  crate in one plane depends on the other, and that no Git or Kubernetes client
  exists **anywhere in the workspace** — including the control plane, which
  reaches its Git host over HTTPS rather than by linking a Git library.
- It also asserts, per ADR 0018, that no workspace crate's non-dev dependency
  closure touches both planes -- catching a bridge through a crate in neither
  one that a direct-edge check, and even an up-to-date `expected` table, would
  miss -- and that no runtime-plane crate can reach
  `fabric-runtime-publication` over any dependency table, so the runtime plane
  can never link a writer of the files it reads.
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

**A new runtime API** (Configuration, Feature, Storage, Events, Experience)
sits at the same level as `fabric-data-api`: depending on core, identity, tenant
runtime and connector, depended on only by `fabric-api`. They must not depend on
each other. If two of them need to share something, that is a signal for a new
shared crate or a core primitive — decided deliberately, not by adding an edge.

**A new control-plane capability** (authorization, secrets, routing,
observability) follows the shape identity established: the concept in
`fabric-client-model`, the convergence semantics in `fabric-reconciliation` or a
sibling, the port beside the existing one, and the platform service's protocol
in an adapter crate of its own that nothing but the composition root can see.

Whichever it is, add it to `expected` in `scripts/check_architecture.py`. A
crate nobody has placed in the graph is a crate whose dependency direction
nothing is checking, and the script fails until someone does.
