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

## The port's contract

Every operation must be **idempotent**: creating a realm, a role, or an
application client that already exists must succeed, not fail. The reconciler
diffs first and does not ask for work it can see is unnecessary, but the two
are separated by a network and by whatever the provider does on its own —
Keycloak creates several roles with every realm — so an adapter that returned
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
contract, and seeds the roles a real provider creates for itself. It is `pub`
rather than `#[cfg(test)]` because the control-plane host uses it as a
development adapter, so `cargo run` needs no Keycloak.
