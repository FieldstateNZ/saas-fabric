# fabric-keycloak

Where SaaS Fabric's identity concepts become Keycloak's, and nowhere else.

```text
IdentityReconciler          realm, role, application client
        ↓
IdentityProvider            the port — still SaaS Fabric's words
        ↓
KeycloakIdentityProvider    ← the translation happens here, and only here
        ↓
Keycloak Admin REST API     RealmRepresentation, ClientRepresentation, …
```

## Keycloak types stop here

`RealmRepresentation`, `RoleRepresentation`, `ClientRepresentation` and the
admin token exchange are `pub(crate)`. Nothing outside this crate can name them,
and `scripts/check_architecture.py` fails the build if anything tries.

That is the same containment ADR 0001 applies to the NDC protocol in the runtime
plane, for the same reason: a representation that escapes its adapter turns the
platform's own model into a thin wrapper over somebody else's — and the operator
console stops being about clients and starts being about realms.

## What it reconciles, and what it leaves alone

Reconciled: the realm, its display name, the required realm roles, and the
declared application clients.

Left exactly as they are: token lifespans, brute-force policy, password policy,
themes, federation, authentication flows, and every other realm setting. Not
because they do not matter, but because SaaS Fabric has no opinion about them —
and a `RealmUpdate` carrying a full representation would reset every one of them
each time a display name changed.

That restraint is expressed in the wire types: the update body has two fields.

## The machine identity

The adapter is handed an `AdminCredential` — a redacting newtype with no
`Display` and a fixed `Debug`. A `String` in the same place is one `{:?}` on a
config struct away from putting the platform's Keycloak administrative secret
into a log aggregator, and the code that leaks it looks exactly like the code
that does not.

`KeycloakConfig::client_secret_ref` names the secret; it never carries one. This
application defines a configuration *contract* — "there is a value called this,
and I need it" — and how it arrives is `saas-fabric-platform`'s decision (§20).

The credential must be a confidential Keycloak client created for SaaS Fabric.
Never a human administrator's password, never a browser session, and never
anything the operator console could supply.

The token is cached for its own reported lifetime less a 30-second margin, and
the lock is held across the refresh so two concurrent sweeps do not both
re-authenticate.

## The permission it needs is one realm role

**`create-realm` on the administrative realm. That is all.**

This was an open question — grant broad master-realm administration, or find
something narrower? — and it was settled by measurement against LucentRoot
rather than by reading. When a service account holding `create-realm` creates a
realm, Keycloak grants *that account* the new realm's full administrative role
set on the corresponding `<realm>-realm` client. So the identity earns
authority over exactly the realms it created, and over nothing else.

Verified on Keycloak 26.7.2: after the service account created `acme`, its
mappings held `manage-realm`, `manage-clients`, `view-realm`, `view-clients`
and the rest — on `acme-realm`, and on no other realm's client.

No master-realm administrator role is needed, and no bootstrap debt is
outstanding.

## The refusal that is not a permissions problem

There is a consequence of the above that only appears against a real Keycloak,
and it cost a failed reconciliation to find.

Those roles are granted into tokens minted **after** the realm exists. The first
pass over a new client mints a token, creates the realm with it, and then tries
to create a role inside it — with the token it is still holding, which was
minted a moment before the realm did. Keycloak answers `403`. The credential is
correct, the grant is correct, and the token is simply too old to know about
either.

So `admin::requests` retries **once** on `401` or `403`, after discarding the
cached token. That turns a client's first reconciliation from "fails, then
succeeds a minute later once the cache expires, for reasons no log explains"
into one extra round trip on the one pass where it matters.

It is once, deliberately. A provider that refuses a freshly minted token is a
misconfigured credential, and retrying that would turn one bad secret into a
request storm.

## Failures, and why three kinds

| Reported | From | Because |
|---|---|---|
| `NotPermitted` | `401`, `403` | the platform's credential is wrong; retrying forever will not fix it |
| `Unavailable` | `5xx`, transport | Keycloak is unwell; the next sweep may find it recovered |
| `Rejected` | other `4xx` | the desired state cannot be realised as written; an operator has to act |

**The response body never appears in any of them.** Keycloak's admin errors
quote realm internals and occasionally echo request content, and this text is
shown to an operator and written to a log. The message is composed from the
operation and the status.

Transport failures are all `Unavailable`, including a timeout that may have
fired after Keycloak had already acted — a deliberate difference from the
runtime plane's connector client, which distinguishes "certainly not applied"
from "outcome unknown" because there the operation may be a write whose
duplicate would corrupt a tenant's data. Here every operation is idempotent by
the port's contract, so re-attempting one is safe by construction.

## Two details that look like bugs and are not

**`409 Conflict` on a create is success.** The port requires idempotence, and
Keycloak creates several roles with every realm.

**A role name that will not parse is skipped.** A *declared* role always parses
— it came from a document this platform validated — so a name that fails is by
definition one SaaS Fabric did not declare and will never look for. Dropping it
changes no decision; keeping it would mean widening `RoleName` to hold values
the platform refuses to write. The name is not logged: it is text from an
external system.

## What it will not do

There is no path in this crate that deletes a realm, a role, or a client. The
port it implements has no such operation.
