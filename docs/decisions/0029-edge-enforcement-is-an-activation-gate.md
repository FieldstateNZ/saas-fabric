# ADR 0029 — Edge enforcement is an activation gate

- **Status:** Proposed
- **Date:** 2026-11-15
- **Applies to:** the tenant runtime in `saas-fabric-platform`
  (`applications/core/saas-fabric`): every route that forwards to its Data
  API, and the replica count of its Deployment
- **Related:** [ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md)
  §1 and §G; [ADR 0022](0022-running-versions-come-from-deployment-evidence.md);
  [ADR 0027](0027-data-path-authorization-is-layered-and-fails-closed.md)
  (Proposed); [#113](https://github.com/FieldstateNZ/saas-fabric/issues/113)

## Context

On the tenant runtime Data API,
[ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md)
makes the edge the only place a token's signature, issuer and audience are
checked. The runtime runs the canonical trusted-ingress reader. It decodes
the payload and checks the validity window. It does not verify the token
itself.

That edge is still absent. Today the Data API route in
`saas-fabric-platform`,
`applications/core/saas-fabric/base/httproute.yaml`, forwards `/v1/data`
with no JWT `SecurityPolicy`. The runtime Deployment stays at zero
replicas. A replica above zero, on that route or on any later route that
forwards to the Data API, would serve tenant data to any caller who can
reach it, including one presenting an unsigned token or a token from an
issuer the route was never told to trust.

The end-to-end tenant request in
[#113](https://github.com/FieldstateNZ/saas-fabric/issues/113) needs a
runtime that is actually serving, so it waits on this gate.

[ADR 0027](0027-data-path-authorization-is-layered-and-fails-closed.md)
(layered data-path authorization) is still Proposed. It decides what an
already-admitted caller may do. It is not required for this gate.

## Decision

No tenant runtime replica may go above 0 until ADR 0019's edge enforcement
is attached to every route that forwards to the Data API, and tested.

**Attached.** A JWT `SecurityPolicy` targets every route that forwards to
the Data API. Today that is `/v1/data` in `saas-fabric-platform`
`applications/core/saas-fabric/base/httproute.yaml`, the route §G1 names.
A route added later falls under the same rule the moment it forwards
there. The policy checks the signature against the issuing realm's JWKS,
exact `iss` membership, audience, a required `exp`, and a per-issuer
algorithm allow-list. `/health` and `/ready` stay outside that path, as
§G1 already requires.

**Tested.** At zero replicas a request the gateway rejects never reaches
the runtime, so the test is not a call against the runtime Deployment. It
runs at the gateway against a stand-in backend, or on a disposable cluster
that stands the gateway up while the tenant runtime stays at zero replicas.

The policy accepts a valid token from the correct issuer. A policy that
rejects every token fails that case. The policy rejects an unsigned token,
a wrong-issuer token, a wrong-audience token, an expired token, and a
token whose algorithm is not allowed, including `alg: none`.

The record is digest-pinned evidence in the sense of
[ADR 0022](0022-running-versions-come-from-deployment-evidence.md). It names
the Envoy Gateway version that enforced the policy, the `SecurityPolicy`
revision that was attached, and the runtime image digest that will be
scaled. The test requests stop at the gateway, so the digest in the record
is the one the later scale-up will run.

The replica count stays 0 until both hold. Raising it is a deliberate
operator action taken after that evidence exists. This decision does not
perform that action.

ADR 0027 stays outside the gate. The first replica may be raised while
layered data-path authorization is still Proposed.

## Consequences

- The tenant runtime remains at zero replicas until a JWT
  `SecurityPolicy` targets every route that forwards to the Data API, and
  the acceptance and the rejections above are recorded with the Envoy
  Gateway version, the `SecurityPolicy` revision, and the runtime image
  digest that will be scaled.
- Platform CI fails when any runtime replica count is above 0 while the
  `SecurityPolicy` is not targeting the Data API route or routes. Detaching
  the policy after scale-up fails that check, so the route cannot be
  reopened quietly. The check is implemented in `saas-fabric-platform`.
- [#113](https://github.com/FieldstateNZ/saas-fabric/issues/113) waits on
  that evidence before an end-to-end tenant request can be served.
- ADR 0027 may be accepted, amended or left Proposed without moving this
  gate.
- This repository still writes no gateway resource, JWT policy, replica
  count or the platform CI check. Those stay in `saas-fabric-platform`,
  behind the security gate ADR 0019 already requires.
