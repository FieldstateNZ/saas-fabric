# ADR 0017 — Fabric decides which client secret boundary an operation reaches

- **Status:** Accepted
- **Date:** 2026-08-31
- **Applies to:** `fabric-control-plane`, `fabric-openbao`, `fabric-client-model`, and the platform's OpenBao policy
- **Related:** [ADR 0008](0008-desired-state-is-the-authority.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md); [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md)

## Context

A client's secrets live in that client's OpenBao **namespace**. The namespace
is the boundary; there is no second partition abstraction layered on top of it.

The platform reaches those namespaces with one credential. Measured against a
real store while building this: policies are looked up in the *token's*
namespace, so a policy granting `secret/*` grants the root namespace and no
client's — every client operation answers `permission denied`. What works is a
single-segment glob:

```hcl
path "+/secret/*" { capabilities = ["create", "read", "update", "delete", "list"] }
```

`+` matches exactly one namespace segment, so this grants every client's
boundary and nothing nested inside one. It also never needs reconciling when a
client is added, which is why it is preferred to enumerating namespaces.

The consequence is the thing worth being explicit about: **the platform
credential is deliberately capable of reaching every first-level client
namespace.** It is not scoped to one client, and no request makes it so.

## Decision

**The Fabric control plane uses a platform OpenBao credential authorised for
first-level client namespaces. For every client secret operation, Fabric SHALL
authorise the operator for the selected client and derive the OpenBao namespace
exclusively from trusted client desired state. Callers SHALL NOT supply or
override namespace, mount, OpenBao address, token, or policy.**

```text
operator asks for Acme
        ↓
Fabric authorises the operator          ← a security boundary
        ↓
trusted desired state resolves Acme → namespace   ← a security boundary
        ↓
platform OpenBao credential
        ↓
+/secret/*
```

### The trust split, stated plainly

> **OpenBao enforces namespace isolation. Fabric enforces which namespace a
> given operator operation may target.**

Both halves are load-bearing and neither substitutes for the other. OpenBao
guarantees that a request carrying `X-Vault-Namespace: acme` cannot see
Contoso's secrets — measured: the same path answers `404` without the header
and in another namespace. It guarantees nothing about *which* namespace Fabric
puts in that header.

That choice is Fabric's, and it is therefore a security boundary rather than
convenience logic. Two properties make it one:

- the namespace is read from the client's desired state, which an operator
  changes through the control plane and Git records — there is no request field
  it can arrive in, and `SecretNamespace` has no deserialisation path from a
  request body
- a client with no declared boundary is **refused**, never defaulted to
  something derived from its id, because a derived boundary is another client's
  boundary the first time the derivation is wrong

### Why not a credential per client

It would narrow the blast radius of a bug in the resolution above, and it would
cost a credential lifecycle per client — issuing, rotating, revoking, and
storing — for a central control-plane component that already authenticates
every operator and already reads every client's desired state. The trust
boundary is clear and enforced in one place; per-client credentials are a
larger change for a marginal narrowing, and are not ruled out later.

## Consequences

**The resolution and the authorisation are the things to review.** A defect in
either reaches every client, which is the price of the broad credential. They
are small, in one place, and covered by tests that assert a traversal cannot
climb out of a boundary and that no secret route answers without an operator.

**A path is the only thing a caller supplies**, so it is validated as such:
absolute paths, empty segments, `.`, `..` and `%` are refused before anything
downstream sees them — the boundary is enforced by prefixing a namespace, and a
path that can climb out of its prefix makes that enforcement decorative.

**Reveal is recorded like any other operation.** Reading a secret is an act,
and the audit record carries actor, client, path, operation, version and
outcome — and can carry no value, because nothing that could contain one is
passed to the function that writes it.

**The platform's policy is part of this decision.** `+/secret/*` belongs in
`saas-fabric-platform` beside the role that carries it, and narrowing it to
enumerated namespaces later is a policy change rather than a code change.
