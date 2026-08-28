# ADR 0008 — Control-plane mutations write desired state; platform services are reconciliation targets

- **Status:** Accepted
- **Date:** 2026-08-28
- **Applies to:** `fabric-control-plane`, `fabric-client-git`,
  `fabric-reconciliation`, `fabric-keycloak`, `apps/control-plane-ui`, and every
  platform capability added after this one
- **Related:** [Platform specification](../architecture/tenant-runtime-data-api.md) §4, §5, §6, §30, §31; [The control plane](../architecture/control-plane.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md)

## Context

The platform needed its first control-plane capability: an operator changing a
client's identity configuration — the realm, its roles, its application clients
— through SaaS Fabric rather than through Keycloak's admin console.

There are two ways to build that, and they look almost identical from the
outside.

**Call the platform service.** The API receives the change and calls the
Keycloak admin API. It is one hop, the operator sees the result immediately, and
there is no reconciliation to write. Git holds a record of intent that some
other process keeps in step.

**Write desired state.** The API receives the change, writes a document to Git,
and something else converges Keycloak onto it. The operator's write returns
before the realm has changed.

The second is more machinery and a worse first impression. It was chosen anyway.

## Decision

**A control-plane mutation writes desired state. It does not call a platform
service.**

Git is the authority. Platform services — Keycloak today, OpenFGA, OpenBao,
Envoy and Grafana later — are **reconciliation targets**, never sources of
truth and never alternative places to write.

Four consequences, each of which is enforced rather than documented:

1. **There is no code path from an HTTP handler to a platform service.**
   `fabric-control-plane` does not depend on `fabric-keycloak`, its router state
   holds no provider, and `scripts/check_architecture.py` fails the build if
   either changes.

2. **A successful write reports `pending`, not `applied`.** Writing the document
   and converging the provider are different events that fail independently, and
   an API reporting them as one would be lying about the second one every time.

3. **Reconciliation is idempotent and additive.** It creates what is missing and
   corrects what it manages. It deletes nothing — see "What this does not
   decide".

4. **Concurrent edits are refused, never merged.** A write states the revision
   it is editing; the Git host applies it only if that revision is still
   current, and answers `409` otherwise. There is no last-writer-wins path.

## Why

### One authority, or two writers racing

If the API writes Keycloak *and* Git records intent, there are two writers and
nothing that makes them agree. The failure is not hypothetical or rare: the
Keycloak call succeeds and the Git write fails, or the reverse, and now the
platform's record of what a client should have disagrees with what it has — with
no way to tell which one is wrong.

Making Git the only writer of intent means the disagreement becomes a *detected*
state (`drifted`, `pending`, `failed`) rather than an invisible one.

### The specification already required it

§6 is explicit that Git defines desired state and that reconciliation produces
runtime state from it. §31 lists "Git is the source of desired tenant state" as
invariant 1. A control plane that wrote Keycloak directly would satisfy neither,
and would make the runtime plane's careful independence from Git a coincidence
rather than a design.

### The audit trail comes free, and is not sufficient alone

A Git-backed authority means every change is a commit: reviewable, revertable,
and attributable. That is genuinely valuable and it is why the commit message
carries a `Requested-by:` trailer.

It is deliberately *not* treated as the whole audit trail. Every commit is
authored by the platform's machine identity, a refused write leaves no commit at
all, and a future repository may not be Git. So the control plane emits its own
structured audit event as well (§24).

### It generalises, and that is the point

Identity is the first capability, not the only one. The same shape — port,
desired state, reconciler, adapter — is what OpenFGA, OpenBao, Envoy and runtime
bindings will each need. Choosing "call the service" for identity would have
meant choosing it five more times, or rewriting it five times.

## Consequences

**An operator's change is not instant, and the console says so.** The
reconciliation badge is the most important thing on the identity screen for
exactly this reason. A sweep runs immediately on an accepted write, so in
practice it is seconds — but the API never claims otherwise, even when
reconciliation is broken.

**Drift becomes visible.** Because reconciliation observes before it acts, a
realm changed outside SaaS Fabric is noticed and reported as `drifted` rather
than silently corrected. Nothing else in the platform could have told an
operator that.

**The control plane is stateless and needs no working copy.** The Git adapter
speaks the hosting provider's contents API over HTTPS, so `git2`, `gix` and
`gitoxide` stay banned workspace-wide — which keeps "Git is never in the request
path" a structural fact about every binary this workspace builds, not just about
the runtime crates.

**Two round trips per edit.** Read the document, then write it. Accepted: the
alternative is a write with no expectation, which is the lost update this
decision exists to prevent.

**The reconciliation status store is in memory, and is lost on restart.**
Acceptable because reconciliation is idempotent and runs on a schedule, so a
restarted control plane rebuilds the truth within one sweep. What is genuinely
lost is *history* — that a client was `drifted` an hour ago — and a durable
store is what a later increment adds when there is somewhere to put it.

## What this does not decide

**Deletion.** Nothing in this increment deletes a realm, a role, an application
client, or a client document. A role that exists is a role something may already
be granted, and "the operator removed a line from a YAML file" is not enough
evidence to revoke it. A deletion path needs its own confirmation semantics and
its own decision.

**Confidential application clients.** They need a client secret, secrets never
enter desired state, and inventing a place for one in the document would be the
first step toward Git holding credentials. Supporting them means designing
secret delivery first.

**Realm renaming.** Refused, because reconciliation only adds: a rename would
create a second, empty realm and abandon the first — with every user, session
and application still in it. There is no safe way to express that as a document
edit, so it is not expressible at all.

**Runtime binding publication.** Named as a future reconciliation target in
[the control-plane architecture](../architecture/control-plane.md), and
deliberately not built. When it is, it belongs behind a port beside the identity
one — not as a control-plane mutation reaching into a runtime registry.
