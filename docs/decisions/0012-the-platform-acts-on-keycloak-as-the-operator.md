# ADR 0012 — The platform acts on Keycloak as the operator

- **Status:** Accepted
- **Date:** 2026-08-29
- **Applies to:** `fabric-control-plane`, `fabric-keycloak`, `fabric-control-plane-api`
- **Related:** [ADR 0009](0009-operator-identity-is-not-tenant-identity.md); [ADR 0010](0010-operators-authenticate-against-the-platform-realm.md); [ADR 0011](0011-the-platform-creates-its-own-git-application.md)

## Context

Two different things point at Keycloak, and conflating them is easy:

```text
Keycloak → SaaS Fabric     operator sign-in — who may administer     (ADR 0010)
SaaS Fabric → Keycloak     the Admin API — creating a client realm   (this ADR)
```

[ADR 0010](0010-operators-authenticate-against-the-platform-realm.md) settled
the first. The second was still a **standing machine credential**: a
confidential client called `saas-fabric` whose service account held
`create-realm`, delivered to the pod by External Secrets and exchanged for an
admin token with `client_credentials`.

That credential could create a realm at three in the morning with nobody having
asked. Its authority was the platform's own and independent of any human's,
which made "who decided this?" a question the platform could not answer about
its own most consequential action.

## Decision

**The platform holds no authority over Keycloak. It acts as the operator who
asked, using the bearer they presented.**

- `IdentityProviderFactory` builds a provider **per operator**, from their
  token. `AdminCredential`, the token cache and the `client_credentials`
  exchange are deleted; `KeycloakConfig` has no credential field.
- `Operator` carries the bearer it was authenticated with, as a redacting
  newtype.
- Permission to create a realm belongs to a person in the master realm, where
  Keycloak's own RBAC already governs it.

### The trusted-header posture is removed, not kept for development

It asserted a name from a proxy header and lent nothing. Two consequences made
keeping it untenable rather than merely untidy: it was safe only because of
*where the service sat*, with nothing in the application enforcing that; and an
operator established that way cannot authorise anything above, so half the
control plane would be unavailable under it.

A development posture that cannot do what production does hides exactly the
failures worth finding early. Local development now needs a Keycloak, and the
shipped example says so.

## Consequences

**There is no unattended convergence.** The interval loop is gone, because
there is no authority available when nobody is there. Convergence happens on a
write, and on demand through `POST /api/reconciliation`.

That is a real loss and it is the trade this ADR argues for. Keycloak's own
console is published on no plane in any environment
([the Keycloak deployment](https://github.com/FieldstateNZ/saas-fabric-platform)
enforces it), so changes made outside SaaS Fabric are largely *prevented*
rather than merely noticed afterwards. Drift detection becomes something an
operator asks for, which is also when anybody is available to act on it.

**An operator needs more than `create-realm`.** This is the sharp edge, and it
is not obvious.

Creating a realm causes Keycloak to grant the creator that realm's
administrative roles — into tokens minted *afterwards*. A service account could
simply mint a fresh one, and the adapter did exactly that: it invalidated its
token and retried once, which is why that retry existed at all.

A borrowed token cannot be re-minted. So an operator whose authority is
`create-realm` alone will create a realm and then be refused on the first role
inside it, with a token that is perfectly valid and simply too old to know the
realm exists. Their authority has to **already cover realms that do not exist
yet** — master-realm `admin` does.

The retry is deleted rather than adapted, because there is nothing to retry
with, and a refusal is now reported honestly instead of worked around.

**`saas-fabric-platform` supplies no Keycloak credential.** The control plane's
`ExternalSecret` goes away entirely: nothing about Keycloak is delivered to the
pod any more. Combined with [ADR 0011](0011-the-platform-creates-its-own-git-application.md),
the deployment now supplies **no credential of any kind** — the Git application
is created in-product and kept in the instance's own secret partition, and
Keycloak is reached with a human's authority.

**The two adapters get their authority differently, deliberately.**
`fabric-client-git` holds a credential the platform owns, because Git holds
*desired state* and the platform must be able to read it to know what it should
be doing. `fabric-keycloak` borrows a person's, because Keycloak is *changed*,
and changing an organisation's identity provider is an act that ought to trace
to somebody who chose it.
