# ADR 0014 — Fabric calls OpenFGA as the operator, in the control plane only

- **Status:** Accepted for the control plane. The runtime's credential is deliberately **not** decided here.
- **Date:** 2026-08-30
- **Applies to:** `fabric-control-plane`, the OpenFGA adapter when it is built, and the platform's Keycloak configuration
- **Related:** [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md); [ADR 0013](0013-authorization-is-declared-in-the-platforms-words.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md)

## Context

[ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md) removed the
platform's standing credential for Keycloak: it acts as the operator who asked,
so permission belongs to a person. The obvious question was whether OpenFGA
could be held to the same rule, or whether adding it would quietly put a
machine credential back into the architecture.

**Two identities are involved and conflating them is the trap.**

```text
1. Who may call the OpenFGA API?      → the bearer token OpenFGA authenticates
2. Who is the check about?            → user + relation + object in the request
```

A token being accepted at the door says nothing about whom the decision inside
concerns. Everything below depends on holding those apart.

## What was measured

Against OpenFGA v1.19.0, a throwaway Keycloak 26 with real users, and the live
LucentRoot realm.

| | result |
|---|---|
| A **person's** token (`sub` = a user, not a service account) | accepted — store created |
| No token / malformed token | 401 |
| Same user, same realm, same signature, without the `openfga` audience | **401** |
| Configured issuer differs from the token's `iss`, no alias | 401 |
| …with the token's `iss` as `--authn-oidc-issuer-aliases` | accepted |
| Caller authenticated from realm A, check subject a `sub` from realm B | **`allowed: true`** |
| Token from a second realm, that realm listed only as an alias | **401** |

OpenFGA validates `iss`, `aud`, signature and expiry. `--authn-oidc-subjects`
defaults to empty and its own help states *"If empty, every `sub` will be
allowed"* — so nothing in it requires a machine subject.

## Decision

**In the control plane, Fabric presents the authenticated operator's own
Keycloak token to OpenFGA.** No standing credential, exactly as ADR 0012.

Two pieces of configuration make it work, and both are configuration rather
than secrets:

- an `oidc-audience-mapper` on the console client adding `openfga` to `aud`.
  This is a virtue, not a workaround: it is an explicit statement that *this
  operator credential may be presented to OpenFGA*, visible in the token, where
  a shared service key would say nothing at all.
- `--authn-oidc-issuer` pointing at the address the OpenFGA pod can reach, with
  the public issuer in `--authn-oidc-issuer-aliases`. Keycloak advertises
  whichever hostname it was reached on, so on LucentRoot the pod discovers
  `http://keycloak-http.identity.svc.cluster.local/realms/master` while
  operator tokens carry `https://fabric-lucentroot.tail5a7546.ts.net/realms/master`.

## The constraint that stops this generalising

**One OpenFGA cannot trust more than one Keycloak realm.**
`--authn-oidc-issuer` is a single value and is where keys are fetched from;
`--authn-oidc-issuer-aliases` accepts alternative `iss` *strings* against that
same key set. Two realms have different signing keys, and a token from the
second is refused even when its issuer is listed as an alias. Measured, not
inferred.

So the runtime plane cannot forward a client user's realm token to a shared
OpenFGA. It does not need to: the caller and the check subject are independent,
so the runtime authenticates **as itself** and names the client's user in the
request. Isolation between clients comes from a store per client, not from
which realm signed the caller's token.

That leaves the runtime's own credential genuinely undecided, and it is a
different problem rather than the same one: a tenant request arrives with no
operator present, at any hour, so there is no delegated authority to borrow.

It does **not** follow that the answer is a stored secret. Two later
measurements narrow it, and both are recorded here because they change what is
possible rather than merely what is convenient.

**The one-issuer limit is per deployment, not per datastore.** Two OpenFGA
deployments sharing one Postgres hold the same stores, models and tuples while
authenticating their callers completely independently — a store created through
one is readable through the other, and the first's credential is refused at the
second's door. So the control plane can keep presenting operator tokens against
the platform realm while the runtime authenticates by some entirely different
means, over the same data. The choice is not either/or.

**Kubernetes is itself an OIDC issuer, and its workload tokens are secretless.**
A projected service-account token carries `iss:
https://kubernetes.default.svc.cluster.local`, an audience bound to whatever is
asked for (`aud: ["openfga"]`), a durable subject
(`system:serviceaccount:<namespace>:<name>`), and a short expiry the kubelet
rotates. Every claim OpenFGA validates is present, and nothing is stored
anywhere.

Two obstacles were measured, one solved and one not. OpenFGA does not trust the
cluster CA by default and panics with `x509: certificate signed by unknown
authority`; pointing `SSL_CERT_FILE` at the service-account CA fixes it. It
then fails with `401`, because this cluster binds
`system:service-account-issuer-discovery` to the `system:serviceaccounts` group
and OpenFGA fetches discovery unauthenticated. Making that path work needs a
deliberate cluster change — binding discovery for unauthenticated callers, or
serving the document to OpenFGA some other way. The material there is public
key material, but the change widens an anonymous surface and belongs to
whoever runs the cluster, not to this ADR.

Recording all of this plainly is the point: the runtime's credential must be
chosen deliberately, and "standing secret" is now demonstrably not the only
option on the table.

## Consequences

**OpenFGA must never be user-facing.** With `subjects` empty, any token bearing
the right issuer and audience is an accepted API caller. Fabric talks to
OpenFGA; browsers and client applications do not. OpenFGA's own access-control
feature could later scope a caller to particular stores, but it is behind the
`enable-access-control` experimental flag in v1.19 and is not something to
depend on yet.

**OpenFGA fails closed on startup.** Pointed at an unreachable or wrong issuer
it panics rather than starting unauthenticated:

```text
panic: failed to initialize authenticator: error fetching OIDC configuration:
unexpected status code getting OIDC: 404
```

Right behaviour, and it makes Keycloak a startup dependency: an OpenFGA restart
during a Keycloak outage will not come back until Keycloak does.

**One unproven step.** Operator-token acceptance was proven with a real user
token against a throwaway Keycloak; the LucentRoot half — discovery, issuer,
alias, real JWKS — was proven in-cluster against the live realm. What OpenFGA
validates is identical in kind, but no *LucentRoot* operator token has yet been
presented to an OpenFGA. One console sign-in and one call would close it.
