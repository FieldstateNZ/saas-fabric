# ADR 0013 — Authorization is declared in the platform's words

- **Status:** Accepted
- **Date:** 2026-08-30
- **Applies to:** `fabric-core`, `fabric-client-model`, `fabric-data-api`, and the OpenFGA adapter when it is built
- **Related:** [ADR 0008](0008-desired-state-is-the-authority.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md)

## Context

The architecture has always named OpenFGA as the implementation of the
**Authorization** capability, beside Keycloak's Identity, and always marked it
*not built*. Two things now make its shape a decision rather than a placeholder.

The Data API already has the seam. `ResourcePermissions::permits` decides
whether an identity may perform an operation, and its own documentation says
what it is not:

> Deliberately simple, and deliberately *not* a policy engine. A real deployment
> will have opinions this cannot express, and the right place for those is a
> policy service the platform calls — not a DSL grown here.

And identity established the shape a capability takes: a section of the client
document, written in SaaS Fabric's vocabulary, converged onto a platform
service by a reconciler, with the service's own protocol confined to one
adapter crate. `IdentityConfiguration` says realm, role and application client.
It does not say `RealmRepresentation`.

The tempting shortcut is to put OpenFGA's authorization model — its DSL, or its
type-definition JSON — straight into the client document. It would work
immediately and it would make the document a thin wrapper over somebody else's
schema, which is the failure ADR 0008 and the NDC containment rule both exist
to prevent.

## Decision

**A client declares resources, the relations that can be held on them, and the
operations each relation permits. Nothing else.**

```yaml
spec:
  authorization:
    resources:
      - resource: customers
        relations:
          - name: viewer
            permits: [read, list]
          - name: editor
            permits: [read, list, create, update]
          - name: owner
            permits: [read, list, create, update, delete]
```

Three consequences follow, and each is the point rather than a side effect.

**The model is desired state; the memberships are not.** This document says a
`customers` resource has an `editor` relation and what an editor may do. It
never says Alice is an editor of anything. That is a fact about tenant data, it
changes constantly, and routing it through a Git commit and a reconciliation
pass would be both slow and wrong. Desired state describes the *shape* of
authorization; the tuples that fill it in are runtime data with a different
lifecycle, a different author, and a different rate of change.

**`OperationKind` moves to `fabric-core`.** The control plane now writes
documents naming operations the runtime plane enforces, and the two planes may
not depend on each other (ADR 0008). A vocabulary duplicated across that
boundary drifts silently: the control plane writes `modify`, the runtime plane
looks for `update`, and nothing fails until a caller is refused something an
operator granted. It goes in the one crate both are allowed to share, beside
`LogicalResourceName` — which is there for exactly the same reason, and which
this document reuses so that a resource has one spelling on both sides.

**Relations are nouns, and permissions are stated separately.** A relation says
how a subject is *related* to a resource; what that permits is a second
question. Keeping them apart means widening what an editor may do does not
require inventing a new word for an editor. It also groups the document the way
an operator thinks: "what is an editor allowed to do" is one edit in one place,
rather than an entry added to five operation lists.

## Consequences

The document stays translatable rather than tied. `viewer`/`editor`/`owner`
with permitted operations maps onto OpenFGA types with direct relations plus
computed `can_<operation>` relations, and it would map onto a different engine
too. That translation is the adapter's job and lives only there, the way
Keycloak's representations live only in `fabric-keycloak`.

An empty or absent section reads as "not managed by the platform", not as "deny
everything". Every document already in a repository predates this section, and
a capability that made them unreadable on the day it shipped would be a
capability nobody could adopt incrementally.

Nothing converges this yet. The section is declared, validated and refused when
wrong; no reconciler reads it and no OpenFGA exists to receive it. That is the
same order identity was built in, and it is deliberate: the vocabulary is the
part that is expensive to change once documents in Git use it, so it is settled
before anything depends on it.

**How the platform authenticates to OpenFGA** was left open here and is now
settled for the control plane by
[ADR 0014](0014-fabric-calls-openfga-as-the-operator.md), which also records
the measured constraint that stops the same answer being reused by the runtime.

**How subjects are named** — the relation between an OpenFGA user and a
Keycloak subject — remains open, and belongs with the enforcement half. ADR
0014 establishes the one thing that had to be true for the question to be
answerable at all: the identity that *calls* OpenFGA and the identity a check
is *about* are independent, so naming a subject is a decision this platform
gets to make rather than one OpenFGA's authentication forces.
