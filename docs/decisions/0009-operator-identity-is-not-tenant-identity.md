# ADR 0009 — Operator identity is separate from tenant identity

- **Status:** Accepted
- **Date:** 2026-08-28
- **Applies to:** `fabric-control-plane`, `fabric-control-plane-api`
- **Related:** [Platform specification](../architecture/tenant-runtime-data-api.md) §8–§12, §24; [ADR 0002](0002-trusted-ingress-is-the-canonical-identity-model.md); [ADR 0008](0008-desired-state-is-the-authority.md)

## Context

The runtime plane already has a working identity model: a bearer token
established at the platform edge carries a `tenant_id` claim, `fabric-identity`
turns it into a `TenantIdentity`, and every Data API handler takes one as an
extractor so it cannot run without a resolved tenant (§10, §11).

The control plane needs to know who is making a change too, for the same
structural reason and one extra one: §24 requires every mutation to be
attributable.

The tempting move is to reuse what exists. A platform administrator is a user
somewhere; give them a token, put a claim in it, and let the same resolver
answer both questions.

## Decision

**Operator identity and tenant identity are separate mechanisms, and neither can
produce the other.**

- `fabric-control-plane` has no dependency on `fabric-identity`, and the
  architecture check refuses one.
- Nothing in the control plane reads a `tenant_id`, a bearer token, or any
  runtime claim.
- Operator authority is never inferred from a tenant, a role within a tenant, or
  anything a client realm can issue.
- `OperatorAuthenticator` is its own seam, and `Operator` is its own type with no
  constructor outside the crate.

The first implementation, `TrustedHeaderOperators`, consumes an identity that the
**operator network boundary** has already established: the control plane is
reachable only from the operator plane, and the proxy in front of it
authenticates the human and states who they are in a header. A non-empty
allowlist then decides which of those identities administer this platform.

## Why

### They are opposites, not variants

A tenant identity is *scoped*: it says "this request is Acme's", and everything
it authorises concerns Acme's own data. Nothing it can do affects anyone else.

An operator identity is *platform-wide*: it says "this person may change any
client's configuration". The blast radius is every tenant on the platform.

Sharing a type or a resolver between the two makes the difference a runtime
property — a claim, a role, a scope — rather than a structural one. And the
failure mode of getting a runtime property wrong here is a tenant administrator
who can reconfigure other tenants.

### A client realm cannot be the source of platform authority

Client realms are created *by* SaaS Fabric, on behalf of clients, and their
contents are a client's business. If operator authority came from one, then:

- creating a client would create a place where platform administrators can be
  minted;
- a client's own administrator would be one role assignment away from the
  platform;
- the control plane could not administer a client whose realm was broken —
  precisely when it is most needed.

The first is the disqualifying one.

### The posture is the same as the runtime's, and so is the obligation

ADR 0002 chose trusted ingress for the runtime plane: the gateway authenticates,
the service consumes the established identity, and the service is
authentication-agnostic. This is the same choice for the same reason, and it
carries the same warning.

**The header is trustworthy because of where the service sits.** Exposing the
control-plane API anywhere reachable from the product edge makes it trivially
forgeable. That is a property of the deployment, not of this code, and it is why
the configuration says `mode = "trusted_header"` explicitly rather than
defaulting to it — a deployment states its posture.

### An empty allowlist is refused rather than interpreted

Being on the operator network establishes *who someone is*. It does not
establish that they administer this platform. An empty list has two plausible
readings — everyone, or no one — and both are wrong often enough that guessing
is not acceptable. `TrustedHeaderOperators::new` refuses it at construction, so
the process fails to start rather than serving under a posture nobody chose.

## Consequences

**The control plane is unusable without an operator plane in front of it.** By
design. Running it exposed is a deployment error, and the platform's `check.py`
in `saas-fabric-platform` is where that becomes enforceable.

**Local development sends the header itself.** The Vite dev proxy adds it, which
is exactly the role the operator-plane proxy plays in production — so the
application code is identical in both environments and there is no "development
mode" that behaves differently from the thing being shipped.

**Operator identity is currently coarse.** Every authenticated operator may do
everything the API offers. There are no per-client permissions, and pretending
otherwise with an authorisation model nothing enforces would be worse than the
honest limitation. What the subject *is* for is attribution: it names the person
in the audit record and in the commit that carries their change.

**A future mechanism is a second implementation, not a rewrite.** When the
platform can issue operator credentials of its own — through Keycloak, or
something else — it implements `OperatorAuthenticator` and no handler changes.
The trait takes a `HeaderMap` and returns an `Operator`, so there is no
signature by which a tenant could be consulted.
