# fabric-reconciliation

Making an identity provider match a client's desired state.

```text
desired Client              fabric-client-model
      ↓
IdentityReconciler          this crate — what should change, and did it
      ↓
IdentityProvider            this crate — the port, in platform vocabulary
      ↓
Keycloak adapter            fabric-keycloak — the protocol, and only there
```

## The split this crate exists to hold

The **reconciler** owns comparison and convergence semantics: what the desired
state is, what the observed state is, which differences matter, in what order
they are corrected, and what the result is called.

The **adapter** owns one provider's protocol.

Neither knows the other's job, and the seam between them is `IdentityProvider`
— a trait written in the platform's words (realm, role, application client),
not Keycloak's (`RealmRepresentation`, `clientScopes`, `attributes`).

## Three properties, stated plainly

**Reconciliation is idempotent.** Running it three times leaves the same
provider state as running it once, and the second and third runs make no calls
that change anything. That is a property of the diff, and it is asserted rather
than assumed — `reconciler_tests` runs a converged client and checks the
provider saw exactly one read and no writes.

**Reconciliation only adds.** Nothing here deletes a realm, a role, or an
application client. A role the document does not mention is left alone, because
a role that exists is a role something may already be granted, and "the
operator removed a line from a YAML file" is not enough evidence to revoke it.
Deletion is a separate decision with its own confirmation path; ADR 0008
records that it is deliberately not made here.

**A successful write is not a reconciled client.** The control plane writes
desired state to Git; this crate makes a provider match it. Different events,
failing independently — and the gap between them is visible rather than hidden.

## The four statuses

| Status | Means | Who fixes it |
|---|---|---|
| `Pending` | Desired state has changed and has not been reconciled since | Nobody — the next pass |
| `Applied` | The provider matches the desired state | — |
| `Failed` | The last pass could not converge it | Depends on the detail |
| `Drifted` | The provider had stopped matching a desired state already converged | Nobody, but somebody should know |

`Drifted` is the one that is easy to leave out and expensive to lack. Without
it, an out-of-band change to a realm that reconciliation quietly corrects looks
exactly like an ordinary pass, so nobody ever learns that something outside
SaaS Fabric is editing the realms it owns.

A pass on its own can only report success or failure. Whether a *successful*
pass that changed something was ordinary convergence or drift depends on
history, so `status::transition` decides it from the previous report — and it
is written down once, there, rather than inside the store.

## What "matches" compares

An existing application client matches its declaration only when all of the
following hold, and any one failing produces `UpdateOidcClient`:

- it is still **public** — a declared client can never be confidential;
- the provider holds **zero** redirect URIs this model could not parse
  (`unmodellable_redirect_uris == 0`) — a value the model cannot read is
  drift, not silence, because it is exactly the out-of-band edit reconciliation
  exists to catch;
- its redirect URIs equal the declared set exactly — a legitimate extra entry
  the provider holds is drift too, the same as a missing one;
- its PKCE challenge method equals the declared one — an attribute the
  provider holds that this model cannot read reads as absent, and absent is
  drift;
- its audience mapper asserts the deployment's configured audience — removed,
  naming a different audience, or duplicated (a client carrying more than one
  mapper reads as having none of them — see `ObservedOidcClient::audience_mapper`),
  all count as drift, because any of them leaves the edge's `aud` check
  refusing every token the client issues;
- it is still **enabled**, and its **standard flow** is still enabled — a
  client switched off, or with authorization-code disabled, by hand answers
  nobody, silently;
- its post-logout redirect attribute still names **every registered URI** —
  narrowed by hand, it is an operator narrowing where a user can land after
  logging out.

**The audience is provider configuration, not a document field.** A client
desired-state document has no field to name its own audience — see ADR 0019
§1/§G5 for why: it is one string per deployment, and a document that could set
its own would be a document that could opt out of the edge's check. This
crate asks the provider for it, through `IdentityProvider::configured_audience`,
rather than duplicating it into a second configuration path — the same object
that writes the mapper is asked what it wrote, so there is exactly one place
this string lives.

## The port's contract

Every operation must be **idempotent**: creating a realm, a role, or an
application client that already exists must succeed, not fail. The reconciler
diffs first and does not ask for work it can see is unnecessary, but the two
are separated by a network and by whatever the provider does on its own — the
provider creates several roles with every realm — so an adapter that returned
an error for "already exists" would make reconciliation flap for reasons no
operator could see.

`observe_realm` returning `Ok(None)` means the realm is absent. That is not an
error, and the distinction is the reconciler's entire first branch: a realm that
does not exist must be created, while a realm that could not be *read* must not
be, because creating one over a realm that is merely unreachable is how a live
realm gets replaced by an empty one.

## The fake

`testing::FakeIdentityProvider` is a fake, not a mock. A mock would let a test
assert that `create_realm_role` was called and stop there — which is exactly
the test that keeps passing after reconciliation stops being idempotent,
because a mock has no state for a second call to observe.

The fake keeps state, answers `observe_realm` from it, honours the idempotency
contract, and seeds the roles a real provider creates for itself. `new()`
asserts a fixed default audience; `with_audience(...)` lets a test configure
one of its own, for the case where a client's observed mapper must disagree
with what the fake itself would write.

It is `pub` rather than `#[cfg(test)]` because `fabric-control-plane`'s own
`tests/` binaries build one directly, across the crate boundary — most
visibly the composed proof that drives a real router through a whole
reconciliation sweep. It is **not** wired in anywhere as a development
adapter: an unconfigured deployment gets no identity provider at all, and
every client simply reports `pending` forever rather than converging against
a fake nobody chose.
