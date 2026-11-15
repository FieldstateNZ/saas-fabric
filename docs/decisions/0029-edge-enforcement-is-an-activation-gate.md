# ADR 0029 — Edge enforcement is an activation gate

- **Status:** Proposed
- **Date:** 2026-11-15
- **Applies to:** the tenant runtime in `saas-fabric-platform`
  (`applications/core/saas-fabric`): every path that forwards to its Data
  API, and the replica count of its Deployment
- **Related:** [ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md)
  §1 and §G; [ADR 0022](0022-running-versions-come-from-deployment-evidence.md);
  [ADR 0023](0023-data-sources-are-environment-desired-state-and-placement-is-recorded.md)
  §5 (D01-18); [ADR 0027](0027-data-path-authorization-is-layered-and-fails-closed.md)
  (Proposed); [#113](https://github.com/FieldstateNZ/saas-fabric/issues/113)

## Context

On the tenant runtime Data API,
[ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md)
makes the edge the only place a token's signature, issuer and audience are
checked. The runtime runs the canonical trusted-ingress reader. It decodes
the payload and checks the validity window. It does not verify the token
itself.

That edge is still absent. In `saas-fabric-platform`,
`applications/core/saas-fabric/base/httproute.yaml` matches
`PathPrefix: /`. It is a catch-all, it carries no JWT `SecurityPolicy`, and
it forwards the Data API together with `/health` and `/ready`. The runtime
Deployment stays at zero replicas. A replica above zero on any path that
forwards to the Data API would serve tenant data to any caller who can
reach it.

The end-to-end tenant request in
[#113](https://github.com/FieldstateNZ/saas-fabric/issues/113) needs a
runtime that is actually serving, so it waits on this gate.

[ADR 0027](0027-data-path-authorization-is-layered-and-fails-closed.md)
(layered data-path authorization) is still Proposed. It decides what an
already-admitted caller may do. It is not required for this gate.

## Decision

Activation means raising the shared tenant runtime Deployment above 0
replicas. It is the same act as
[ADR 0023](0023-data-sources-are-environment-desired-state-and-placement-is-recorded.md)
§5 (D01-18): one Deployment serves many tenants (ADR 0019 G1), and
onboarding a tenant leaves that count untouched. This edge evidence is an
additional precondition beside §5's `documents`, `identity` and
`connectors` report. §5's `identity` gate is a published issuer registry
and observes no edge application. This decision does not replace that
report.

No replica of that shared runtime may go above 0 until ADR 0019's edge
enforcement is attached to every path that forwards to the Data API, and
tested. This gate does not replace
[ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md).
Every §G row still applies.

**Attached.** A JWT `SecurityPolicy` targets every path that forwards to
the Data API. Today's object is the catch-all above, so attaching the
policy to it as it stands would authenticate `/health` and `/ready`. The
platform enforcement change gives the Data API its own path match — a
`/v1/data` prefix, or a split of the catch-all — and attaches the policy
there. `/health` and `/ready` stay unauthenticated, as §G1 requires. A
path added later falls under the same rule the moment it forwards to the
Data API.

The policy checks the signature against the issuing realm's JWKS, exact
`iss` membership of the registered issuer set, the deployment audience, a
required `exp`, and a per-issuer algorithm allow-list.

**Pinned for the test.**

| Fact | Value |
|---|---|
| Issuer set | The exact issuer strings registered for the tenants this shared route serves (ADR 0019 G4). The record names the set. |
| Audience | The one deployment audience string, `saas-fabric-data-api` (ADR 0019 G5). |
| JWKS | For each registered issuer, the JWKS URI that issuer's gateway registration fetches (ADR 0019 G2). A valid token is signed by a key in that document. The record names the URI. |

Wrong-issuer means an issuer outside that set (G4). A validly signed token
from another tenant whose issuer is in the set passes the edge. Which
tenant an issuer names is the runtime's binding (ADR 0019 §2).

**Tested.** At zero replicas a request the gateway rejects never reaches
the runtime. The test runs at the gateway against a stand-in backend. A
disposable cluster is the same test and still includes that stand-in. A
`503` from an empty upstream is not a pass.

Accepted means the gateway forwarded the request and the stand-in answered
`2xx`. A missing `401` is not acceptance.

Every credential rejection is `401` with
`WWW-Authenticate: Bearer error="invalid_token"`, no body that names the
failed check, and never a `302` (ADR 0019 G8 and G12). The edge's clock
leeway is at most 30 seconds (G7).

| Case | Outcome |
|---|---|
| Valid token: issuer in the set, `aud` `saas-fabric-data-api`, signature from that issuer's JWKS, `exp` inside the leeway, allowed algorithm | `2xx` from the stand-in |
| Same, from a second registered tenant's issuer | `2xx` from the stand-in |
| Unsigned | `401`, as above |
| Wrong-issuer: issuer outside the registered set | `401`, as above |
| Wrong audience | `401`, as above |
| Expired, past the 30-second leeway | `401`, as above |
| `alg: none` | `401`, as above |
| HS256, or any other HMAC | `401`, as above |
| Missing bearer | `401`, as above |
| Missing `iss` | `401`, as above |
| Missing `exp` | `401`, as above |
| Unknown `kid` against a fresh successful JWKS snapshot | `401`, as above |
| Unknown `kid` when the snapshot is not a fresh success | `503`, `Retry-After`, body does not describe the credential, never `401` (G8, G13) |
| JWKS unreachable | `503`, as the row above |
| Caller sends `X-Tenant-Id`, an `x-jwt-claim-*` header, `x-forwarded-user`, an `x-auth-request-*` header and the operator-plane identity header, with an otherwise valid token (G9) | `2xx` from the stand-in, and none of those headers is forwarded. The strip is by prefix, on every request, not by an enumerated list |
| No claim projection (G9a) | `2xx` from the stand-in. The forwarded request carries `Authorization` and no header written from a verified claim: no `claim_to_headers` mapping, no `x-jwt-claim-*` emission, no header of the deployment's own naming. `Authorization` is the only identity-shaped header that crosses |
| Valid token (G10) | `2xx` from the stand-in, and `Authorization` arrives byte-for-byte: no rewrite, no re-mint, no re-sign |
| Direct request from a scratch pod in another namespace to the runtime pod IP, and the same request to its Service (G11) | Connection refused or reset at the network layer, not an HTTP status, for both. The record shows the two requests were blocked. A `NetworkPolicy` object being present is not that evidence. The record names the CNI or network-policy controller that enforced the block. k3s, for example, enforces `NetworkPolicy` only through its built-in kube-router controller, and some installs disable that controller |

These cases are required for the gate. They are implemented in the
platform enforcement change in `saas-fabric-platform`.

The record names the Envoy Gateway version that enforced the policy, the
`SecurityPolicy` revision that was attached, and the runtime image digest
that will be scaled, together with the issuer set, audience and JWKS URIs
above. The test requests stop at the gateway, so the digest in the record
is the one the later scale-up will run
([ADR 0022](0022-running-versions-come-from-deployment-evidence.md)).

QA sign-off on the platform enforcement change accepts that record.
Raising replicas stays the deliberate operator action in ADR 0023 §5
(D01-18). The product owner authorises it, via the PM, under the existing
deploy approval, and only after that sign-off. This decision does not
raise replicas. Platform CI checks that the policy targets the Data API
paths. It does not check that the Tested record exists; that half of the
gate is the sign-off.

ADR 0027 stays outside the gate. The first replica may be raised while
layered data-path authorization is still Proposed.

## Consequences

- The shared runtime stays at zero replicas until a JWT `SecurityPolicy`
  targets every path that forwards to the Data API, probes stay
  unauthenticated, and QA has accepted the Tested record for the cases
  above.
- Platform CI fails when any runtime replica count is above 0 while the
  `SecurityPolicy` is not targeting the Data API path or paths. Detaching
  the policy after scale-up fails that check. The check is implemented in
  `saas-fabric-platform`. It does not stand in for the QA sign-off.
- [#113](https://github.com/FieldstateNZ/saas-fabric/issues/113) waits on
  that evidence before an end-to-end tenant request can be served.
- ADR 0027 may be accepted, amended or left Proposed without moving this
  gate.
- This repository still writes no gateway resource, JWT policy, replica
  count or the platform CI check. Those stay in `saas-fabric-platform`,
  behind the security gate ADR 0019 already requires.
