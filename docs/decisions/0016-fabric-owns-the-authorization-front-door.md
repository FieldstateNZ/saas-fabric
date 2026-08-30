# ADR 0016 — Fabric owns the authorization front door

- **Status:** Accepted
- **Date:** 2026-08-30
- **Applies to:** a new `fabric-fga-auth` component, the `fabric-openfga` distribution image, and the runtime plane's enforcement path
- **Related:** [ADR 0013](0013-authorization-is-declared-in-the-platforms-words.md); [ADR 0014](0014-fabric-calls-openfga-as-the-operator.md); [ADR 0015](0015-a-subject-is-named-by-its-realm.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md)

## Context

A tenant user's token is issued by their own client realm. Every client has a
realm, so the runtime plane faces many issuers with many key sets.

**OpenFGA trusts exactly one issuer.** `--authn-oidc-issuer` is a single value
and is where keys are fetched from; `--authn-oidc-issuer-aliases` accepts other
`iss` strings against that *same* key set. A token from a second realm is
refused even with its issuer aliased (ADR 0014, measured).

**Topaz solves that and imposes a worse constraint.** Its `jwt.allowed_issuers`
is a list, it builds an issuer→JWKS map, and it validates each token against
its own issuer's keys — exactly the shape wanted. But its OIDC client applies a
hard-coded SSRF filter:

```go
if ip.IsLoopback() || ip.IsPrivate() || ip.IsUnspecified() || ip.IsLinkLocalUnicast() {
    continue // Skip unsafe IPs.
}
```

Every Kubernetes ClusterIP is private, so Topaz cannot fetch discovery from an
in-cluster Keycloak. Adopting it would mean designing Fabric's networking
around an authorization engine's defensive assumption — making Keycloak
publicly routable, or leaning on `100.64/10` happening not to satisfy Go's
`IsPrivate()`. That is backwards.

Its empty-`allowed_issuers` behaviour is also unsafe — with no issuers
configured it validates expiry and **not the signature**, so a token whose
signature is the literal bytes `not-a-real-signature` resolves its subject
(measured). That one is fixable by refusing to start; the network constraint is
not.

## Decision

**Fabric owns the authorization front door.** A small component,
`fabric-fga-auth`, sits in front of an OpenFGA that is not reachable by anyone
else, and is shipped with it as one `fabric-openfga` image.

```text
Authorization: Bearer <the user's own JWT>
          │
          ▼
   fabric-fga-auth            unverified `iss`, used only to select a registry entry
          │                   trusted issuer registry  →  issuer-specific JWKS
          │                   verify signature, iss, aud, exp, nbf, alg
          ▼
   principal = <realm>/<verified sub>        ← ADR 0015
   store     = that issuer's configured store
          │
          ▼
   openfga  127.0.0.1, authn=none
```

### It binds identity; it does not merely validate a token

This is the requirement that makes it worth building rather than configuring.
A proxy that only checks the bearer would still pass this through:

```text
JWT:      iss = …/realms/acme,  sub = alice
request:  user = user:bob,  relation = can_edit,  object = thing:123
```

Alice's token is valid and the question asked is about Bob. For a **decision**
operation that is the failure being designed out: the subject being evaluated
is the authenticated caller, and the front supplies it.

### It is not an OpenFGA proxy, and must not promise API compatibility

A single binding rule — "rewrite `user` to the caller" — is wrong the moment
relationships are administered. Alice may legitimately be a realm
administrator granting Bob access:

```text
authenticated caller = Alice
tuple being written  = user: Bob, relation: editor, object: document:123
```

Rewriting `Bob` to `Alice` there would be a bug, not a safeguard. There are
three surfaces and they do not share a rule:

| surface | operations | how the caller relates to the subject |
|---|---|---|
| **Decision** | `Check`, `BatchCheck`, `ListObjects`, `ListUsers` | the subject **is** the caller; the front supplies it |
| **Relationship management** | write, delete tuples | the caller *administers* a relationship about somebody else |
| **Administration** | stores, models, assertions, configuration | not on this path at all (ADR 0014's flow) |

Relationship mutation, if Fabric ever exposes it, is an explicit operation with
its own shape — verify Alice, **authorize Alice to administer access**,
validate the requested mutation, then write. Alice remains the caller
throughout; Bob is the subject of the relationship being administered, never
the principal.

Hence the rule:

> The Fabric authorization front SHALL expose only explicitly supported
> authorization operations. Each supported operation SHALL define how the
> authenticated principal, the tenant/store, and any request-supplied subjects
> are interpreted. Unsupported OpenFGA operations and unhandled request shapes
> SHALL fail closed.

"Wrap OpenFGA's API" is explicitly **not** the goal. A few hundred lines is
believable for the first useful authorization surface and is not a claim about
OpenFGA's full surface.

### The registry separates the logical issuer from where keys are fetched

```yaml
issuers:
  - tenant: acme
    issuer: https://identity.fabric.example/realms/acme        # must equal JWT `iss`
    audience: workspec
    jwks_uri: https://keycloak.identity.svc.cluster.local/…    # trusted, may be private
    algorithms: [RS256]                                        # pinned; see below
    openfga_store: 01ABC…
    authorization_model_id: 01DEF…                             # pinned; never "latest"
```

**The authorization model is pinned too, for the same class of reason.** The
authorization service uses its *most recent* model when a request names none,
which would mean writing a model deploys it — a new one would change runtime
decisions before Fabric had intentionally switched. Models are immutable
versions, so naming one makes deployment a deliberate step:

```text
write the new immutable model → validate it → change the configured model id
                                            → the runtime begins using it
```

The store and the model are both authorization *routing* rather than identity,
and both come from the registration the verified issuer selected. Neither is
ever caller-supplied, and the port an adapter implements cannot omit the model,
so no implementation can quietly fall back to "latest".

**Algorithms are pinned per issuer.** The JWT header does not get to say what
cryptography we are willing to trust. Rejecting `alg: none` is not sufficient:
anything outside the registry's configured set is refused even when the library
could validate it, which is what makes algorithm substitution a configuration
question rather than a parsing one.

The browser sees the public issuer; the verifier fetches keys in-cluster. This
is better than discovery for Kubernetes, and it is *why* it does not reintroduce
the risk Topaz's filter defends against: `jwks_uri` comes from Fabric's own
configuration and never from a claim in an incoming token. Nothing an attacker
controls selects a URL to fetch.

## What the front produces, and where each field comes from

```text
VerifiedIdentity {
    tenant,     // registry-derived
    subject,    // the verified token's `sub`
    principal,  // SubjectId::from_verified(realm, subject)   — ADR 0015
    store,      // registry-derived
}
```

**Only `subject` comes from token claims.** `tenant`, the realm identity and
`store` come exclusively from the registry entry selected by the verified
`iss`. A token carrying plausible `tenant`, `realm`, `store_id` or `principal`
claims is not wrong to hold them — they are simply never read.

## Three failure classes, and they are not the same status

A Keycloak outage must not tell a legitimate user their credentials are
invalid, and an operator must be able to tell the three apart from the status
alone.

| class | causes | result |
|---|---|---|
| **configuration** | zero issuers; duplicate issuer; invalid registry entry | **refuse to start** |
| **credential** | unknown issuer; malformed token; wrong audience; bad signature or key; expired or not-yet-valid; disallowed algorithm | **401** |
| **verification infrastructure** | trusted JWKS endpoint unavailable; refresh failed and no usable cached key | **503** |

### JWKS rotation, specified rather than left to a library

```text
known kid
  → verify locally with the cached key
      valid   → authenticated
      invalid → 401

unknown kid
  → one coalesced refresh for that issuer
      key found          → verify normally
      key still absent   → 401
      refresh unavailable → 503
```

A cached key keeps working while JWKS is unreachable — that is the point of
caching, and it is what stops a provider blip becoming an outage. A refresh
failure **must never** be permission to try another key, skip a check, or
otherwise weaken verification: it is an availability problem, and turning it
into an authentication bypass is the failure this section exists to prevent.

**Bounded staleness.** A cached key set has a maximum age. Without one, a long
provider outage leaves a *removed* signing key trusted indefinitely — a
revoked key is the case where "keep serving" is the wrong instinct. Past the
bound, verification answers `503` until trust can be refreshed, rather than
continuing on keys nobody has confirmed.

**Refreshes are coalesced per issuer.** An unknown `kid` triggers at most one
in-flight refresh for that issuer; concurrent requests wait on it rather than
each starting their own. Without that, an attacker sends a few thousand random
`kid` values and the verifier becomes a JWKS-fetch amplifier pointed at
Keycloak — the authorization front DoSing the identity provider it depends on.

## Invariants

Not deployment advice — properties `scripts/check_architecture.py` and the
component's own tests must hold:

- zero configured issuers → **the process refuses to start**
- unknown issuer → `401`; invalid signature, audience, expiry, not-before or algorithm → `401`
- JWKS unreachable with no usable cached key → `503`, never `401`
- algorithms are pinned per issuer; anything outside the configured set is refused
- the authorization model is pinned per issuer; a registration without one is
  refused at startup, and no adapter may resolve "latest"
- the issuer determines the tenant and the OpenFGA store
- verified `iss` + `sub` determine the principal
- the caller cannot override the principal
- the caller cannot override the tenant or store
- OpenFGA listens on `127.0.0.1` only (`--http-addr`, `--grpc-addr`)
- OpenFGA therefore runs `--authn-method=none` and is never network-reachable
- **all** access to the embedded OpenFGA — runtime authorization and
  control-plane administration alike — traverses an explicitly supported
  Fabric operation; OpenFGA is never independently addressable outside the
  container boundary
- runtime and control-plane operations are **distinct surfaces** with
  independent authentication and authorization contracts, and neither offers
  generic passthrough to the OpenFGA API
- the user's own JWT remains the credential at the Fabric boundary
- **no workload identity exists anywhere in the authorization path**

That last one is the point of the whole design. ADR 0014 left the runtime's
credential open because a tenant request arrives with no operator present. This
answers it by removing the question: the runtime does not need a credential of
its own, because the user's token is carried to the boundary and verified
there.

## Consequences

**We do not fork OpenFGA.** A distribution image with a front process beside an
unmodified OpenFGA is a few hundred lines of security-sensitive code we own,
rather than a permanent fork of a codebase we do not.

**The shape of each operation differs, which is why the rule above is per
operation.** `store_id` is a path segment; `user` is `tuple_key.user` for
Check, a top-level `user` for ListObjects, and one per tuple for Write. A
generic pass-through cannot bind identity correctly for all three, and the one
it gets wrong is the one that matters.

**Administration goes through Fabric too, on a second surface.** An earlier
draft said store administration stayed on ADR 0014's path while OpenFGA bound
loopback — which cannot both be true, because a control plane in another pod
cannot reach a loopback listener. The answer is not an administrative listener
on OpenFGA: that would trade away the invariant this design just established.
OpenFGA keeps exactly **one** door, and the front exposes **two surfaces**
behind it.

```text
                    fabric-openfga image
┌──────────────────────────────────────────────────┐
│                                                  │
│  Runtime surface           Control-plane surface │
│  :8080                     :8081                 │
│  client-realm user JWT     operator identity     │
│  Check, ListObjects, …     stores, models, …     │
│         └──────── explicit operations ────────┐  │
│                                               ▼  │
│                                         OpenFGA  │
│                                     127.0.0.1    │
│                                     authn=none   │
└──────────────────────────────────────────────────┘
```

Separate **listeners**, not `/runtime/*` and `/admin/*` on one port, so
Kubernetes can enforce the distinction as well: the runtime Service publishes
`:8080`, while `:8081` is internal and can be restricted by NetworkPolicy to
the controller that needs it. That is defence in depth and nothing more —
**network isolation is not the authorization mechanism**, and both surfaces
authenticate unconditionally.

The two contracts are deliberately different:

| | runtime surface | control-plane surface |
|---|---|---|
| authenticates | a client-realm user's own JWT, via the issuer registry | the operator model of ADR 0014 |
| tenant and store | from the verified issuer's registration | named by the administrative operation |
| the subject | **is** the authenticated principal | defined per operation, and need not be the caller |
| operations | `Check`, `ListObjects`, … | create store, write model, bootstrap relationships |

The runtime surface never substitutes a workload or service identity for the
user's. The control-plane surface is not constrained by "request subject =
authenticated principal", because an operator administering access is
legitimately acting about somebody else — which is the distinction the
Alice-grants-Bob case above exists to make.

**It is a Fabric primitive, not an authorization system.** Parse `iss`
untrusted, look it up in a trusted registry, verify with that issuer's keys,
bind the verified identity, forward. The authorization decision remains
OpenFGA's.
