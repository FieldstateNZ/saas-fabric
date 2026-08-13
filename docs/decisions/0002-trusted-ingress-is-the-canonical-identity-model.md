# ADR 0002 — Trusted ingress is the canonical identity model

- **Status:** Accepted
- **Date:** 2026-08-12
- **Applies to:** `fabric-identity`, and the deployed posture of every runtime service
- **Related:** [Platform specification](../architecture/tenant-runtime-data-api.md) §8, §9, §11, §12, §24

## Context

The runtime plane needs a tenant identity for every request. The specification
is unambiguous about where it comes from:

```text
internet
   → Envoy / gateway authenticates the caller and validates the bearer
   → ────────── platform trust boundary ──────────
   → SaaS Fabric consumes the established identity
   → tenant_id is read from the bearer token
```

§9 states the invariant plainly: a request accepted by the runtime has already
passed through a trusted platform ingress that authenticated the caller and
validated the bearer token. §24 requires the runtime to stay independent of any
identity-provider implementation.

During the initial build we shipped signature verification as the *recommended*
default, reasoning that §11's ban on `X-Tenant-Id` only bites if the token is
trustworthy inside the trust boundary — otherwise anything that reaches a
runtime pod can mint `{"tenant_id": "globex"}`.

That reasoning describes a real failure mode, but it draws the wrong conclusion,
and this ADR corrects it.

## Decision

**`TrustedIngressReader` is the default and the canonical architecture.**
`ValidatingReader` remains available as an explicitly configured
defence-in-depth mode. Correctly configured trusted ingress produces no warning.

## Why the earlier reasoning was wrong

The scenario — something inside the cluster reaching the runtime with a forged
token — is a **network policy failure**, and signature verification is not its
remedy.

Consider what verification actually buys in that scenario. An attacker who can
reach a runtime pod directly has bypassed the ingress. Verification stops them
forging *that particular* claim, and stops nothing else: they still have
unmediated network access to a service that assumes it is behind a boundary,
including its health endpoints, its metrics, and every other internal API in the
plane. Verifying one hop closes one door in a building with no walls.

Worse, it makes the missing boundary *harder to notice*. A platform that
verifies tokens everywhere looks safe while its network policy is absent, and
the absence is discovered later and elsewhere.

§9 already names the correct control, and names several implementations of it:
`NetworkPolicy`, private cluster networking, service mesh policy, workload
identity, mTLS, ingress-only exposure. That is where the guarantee lives.

There is a second cost. Making verification the default drags
identity-provider responsibilities into the runtime — issuer discovery, JWKS
lifecycle and rotation, realm and audience knowledge — which is exactly the
coupling §24 exists to prevent. Every one of those is a thing that can break, be
misconfigured, or need to change when the identity platform changes, in a plane
whose defining property is supposed to be that it does not care.

## What each posture does

| | `TrustedIngressReader` (default) | `ValidatingReader` (opt-in) |
|---|---|---|
| Parses claims | yes | yes |
| Checks `exp` | yes | yes, and **requires** it |
| Checks `nbf` | yes | yes |
| Verifies signature | no | yes |
| Checks `iss` / `aud` | no | only when configured — and then the claim is mandatory |
| Issuer discovery | never | never |
| JWKS fetching | never | never — snapshot loaded at startup |
| Realm knowledge | never | never |

Expiry and not-before are checked in both. Replaying a captured expired token
is cheap and refusing it costs one integer comparison, so there is no posture
in which accepting one is right; the same holds for honouring a token minted
for later use. Both postures run the same check, so the defence-in-depth mode
cannot end up laxer than the canonical one no matter what the underlying
library can or cannot parse.

The two do differ on a *missing* `exp`. The canonical posture accepts a token
without one — the edge already decided that token was good, and this reader's
job is to consume that decision, not to re-legislate the token's shape. The
defence-in-depth posture requires it, because a bearer token that never
expires is precisely the thing a deployment opts into that mode to refuse.
Rejecting more is the permitted direction between these two; accepting more is
not.

`iss` and `aud` were the reverse of that, and it took an adversarial review to
notice. Configuring an allowlist used to set the *comparison* without making
the claim required, so any token that simply omitted `iss` sailed past an
issuer allowlist — a security control that silently did nothing. Configuring
one now makes the claim mandatory. Leaving it unconfigured means the claim is
not examined at all, which is what an operator who did not set it expects; the
previous behaviour rejected every token that merely *carried* an `aud`, which
made the mode unusable against a real identity provider for reasons nothing
explained.

Neither reader fetches anything. Even in defence-in-depth mode, keys arrive as a
`VerificationKeys` snapshot built outside the request path, and rotation means
building a new one — the runtime does not own the key lifecycle.

## When defence in depth is reasonable

- A regulated environment where an auditor expects verification at more than one
  hop, independent of whether it adds security.
- A migration period in which the ingress guarantee is not yet fully trusted.
- A deployment where some runtime services are, for now, reachable from a
  broader network than the target architecture intends — as a *temporary* measure
  alongside fixing the boundary, not instead of it.

In all three the network policy remains the primary control.

## Consequences

### Good

- The deployed architecture matches the specified architecture. §9 says the
  runtime is authentication-agnostic, and now it is.
- No identity-provider lifecycle in the runtime by default: no JWKS files to
  mount, rotate, or fail to rotate; nothing to change when the identity platform
  does.
- Starting the platform needs no key material.
- The security boundary is in one place, so it is auditable in one place.

### Bad, and accepted

- **A network policy failure is exploitable.** With no boundary, a caller that
  reaches the runtime directly can present any claims it likes. This is accepted
  because it is true of the whole plane regardless of token handling, and
  because the mitigation is the boundary — see the operational requirement below.
- **The runtime cannot detect its own exposure.** A process has no way to know
  whether it is reachable from an untrusted network. That check belongs in
  deployment validation and cluster policy testing.

## Operational requirement

A deployment running the default posture **must** enforce that protected runtime
APIs are unreachable through an untrusted path. This is not advisory; it is the
other half of the invariant this ADR relies on. §9 lists the acceptable
mechanisms. Deployment tooling should verify it, because the runtime cannot.

## Invariants unchanged by this decision

1. Tenant identity comes from the canonical bearer-token claim (§10).
2. No `X-Tenant-Id`, and no other independently selectable tenant mechanism
   (§11). This is orthogonal to signature verification and remains enforced.
3. Parsing claims does not make the runtime responsible for authentication
   (§12).
4. The runtime holds no identity-provider-specific logic (§24).
5. Failure to establish a tenant context rejects the request (§28).

## Supersedes

The identity guidance in the initial implementation, which presented
`ValidatingReader` as recommended and emitted a startup warning for trusted
ingress. That warning is removed: a correctly configured deployment is not a
problem to flag.
