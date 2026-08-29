# ADR 0010 — Operators authenticate against the platform's own realm

- **Status:** Accepted
- **Date:** 2026-08-29
- **Applies to:** `fabric-control-plane`, `fabric-keycloak`, `fabric-control-plane-api`, `apps/control-plane-ui`
- **Related:** [ADR 0002](0002-trusted-ingress-is-the-canonical-identity-model.md); [ADR 0008](0008-desired-state-is-the-authority.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md)

## Context

[ADR 0009](0009-operator-identity-is-not-tenant-identity.md) established that
operator identity is its own mechanism, and the first increment implemented it
as `TrustedHeaderOperators`: the operator-plane proxy authenticates the human,
states who they are in a header, and an allowlist in configuration decides
which of them administer the platform.

That was the right first move and it has two properties worth being honest
about.

**It is only safe because of where the service sits.** The header is
trustworthy because the control plane is reachable from the tailnet and nowhere
else. Nothing in the application enforces that. Publish the same container on
any other network and every request is an unauthenticated request that can
claim to be anybody, with no code change and no failing test.

**Authority lives in a deployment.** Adding or removing an operator is an edit
to a config file in `saas-fabric-platform`, reviewed and rolled out by the same
pipeline that ships the application. Joiners and leavers are handled somewhere
else entirely, and the two drift.

Meanwhile the platform already runs an identity provider, and SaaS Fabric is
its administrative control plane. Asking operators to be authenticated by
something else, while the platform's own identity provider sits beside it, is
the sort of inconsistency that is easy to live with and hard to justify.

## Decision

**An operator is somebody the platform's own realm authenticated, holding a
realm role.** The control plane verifies a token; it does not consult a list of
names.

- `OidcOperators` verifies the token's signature against the realm's published
  keys, requires the issuer to match exactly, requires the token to have been
  issued to the console's client, and requires the configured realm role.
- The keys are held in memory and refreshed by a background task. Verification
  performs no I/O, so the extractor stays synchronous — the runtime plane made
  the same call for the same reason, and this ADR is the record that the
  agreement is deliberate rather than coincidental.
- `TrustedHeaderOperators` remains, as **the development posture**. The shipped
  example uses it, because the example must run without a cluster (§22) and
  OIDC cannot.

### The console never meets the identity provider

The console's content security policy is `default-src 'self'` and its contract
is that it talks to the control-plane API and nothing else. A browser may
navigate to another origin, but it may not `fetch` one — so the console cannot
redeem an authorization code itself.

So it does not. It asks the API where to sign in, sends the browser there, and
posts the returned code back to `/api/session`, which redeems it server-side.
The console holds no client secret because it is a public client; PKCE is what
replaces one, and the browser keeps the verifier.

The token lives in a module variable for the life of the tab. Not
`localStorage`, not a cookie, and no refresh token: a reload signs in again,
which the provider's own session makes near-invisible.

### `azp`, not `aud`

A realm mints access tokens whose audience is the resource server the caller
asked for — commonly `account` — and names the client that obtained the token
in `azp`. Requiring `aud` to equal the console's client id therefore refuses
every genuine token until somebody adds an audience mapper, and the failure
presents as a signature problem rather than a missing mapper.

`azp` answers the question actually worth asking: was this token obtained by
the console, or by another client in the same realm whose holder is now
presenting it here?

## Consequences

**The control plane no longer depends on being unreachable to be safe.** The
operator plane stays a tailnet, because defence in depth is worth having — but
it is now the second line rather than the only one.

**Operator authority moves to the identity provider.** Granting the role is
where joiners and leavers are already handled. Nothing about who administers
the platform lives in a deployment any more.

**The realm needs configuring before the posture works.** A public client for
the console and a realm role are prerequisites; see
[the control-plane architecture note](../architecture/control-plane.md).
Automating that in the reconciler is a plausible next step and deliberately not
in this change — it needs a broader grant on the master realm than the
platform's service account holds today, and that grant deserves its own
decision rather than arriving as a side effect of this one.

**A provider outage does not stop the control plane.** It starts without
reaching the provider, keeps the keys it already has when a refresh fails, and
refuses operator requests only when it has never held a key. The alternative —
refusing to start — makes the console useless for diagnosing precisely the
outage that caused it.

**Two things verify JWTs in this workspace and they do not share code.** The
planes share only `fabric-core` (ADR 0008), and the runtime's reader is a
runtime-plane crate. Both delegate the cryptography to `jsonwebtoken` and
neither hand-rolls verification, so what is duplicated is configuration rather
than security-critical logic. If a third consumer appears, extracting the
shared part into `fabric-core` is the answer; two is not yet enough to justify
moving a well-tested runtime path.
