# ADR 0029 — Edge enforcement is an activation gate

- **Status:** Proposed
- **Date:** 2026-11-15
- **Applies to:** the tenant runtime in `saas-fabric-platform`
  (`applications/core/saas-fabric`): the Data API `HTTPRoute` and the
  replica count of its Deployment
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

That edge is still absent. The Data API route in `saas-fabric-platform`,
`applications/core/saas-fabric/base/httproute.yaml`, forwards `/v1/data`
with no JWT `SecurityPolicy`. The runtime Deployment stays at zero
replicas. A replica above zero on that route would serve tenant data to any
caller who can reach it, including one presenting an unsigned token or a
token from an issuer the route was never told to trust.

W4, the saas-fabric build split, can produce a runtime image that is ready
to ship. Shipping the image and serving traffic are separate. The
end-to-end tenant request in
[#113](https://github.com/FieldstateNZ/saas-fabric/issues/113) needs a
runtime that is actually serving, so it waits on this gate.

[ADR 0027](0027-data-path-authorization-is-layered-and-fails-closed.md)
(layered data-path authorization) is still Proposed. It decides what an
already-admitted caller may do. It is not required for this gate.

## Decision

No tenant runtime replica may go above 0 until ADR 0019's edge enforcement
is attached and tested.

**Attached.** A JWT `SecurityPolicy` is bound to the Data API route
`/v1/data` in `saas-fabric-platform`
`applications/core/saas-fabric/base/httproute.yaml`, the one route §G1
names. It checks the signature against the issuing realm's JWKS, exact
`iss` membership, audience, a required `exp`, and a per-issuer algorithm
allow-list. `/health` and `/ready` stay outside that path, as §G1 already
requires.

**Tested.** The attached policy has been shown to reject an unsigned token
and a wrong-issuer token. The record of that test is digest-pinned evidence
in the sense of
[ADR 0022](0022-running-versions-come-from-deployment-evidence.md): it names
the runtime image digest the requests reached and the gateway configuration
they passed. A commit that adds the policy is desired state. Desired state
does not establish what served the request.

The replica count stays 0 until both hold. Raising it is a deliberate
operator action taken after that evidence exists. This decision does not
perform that action.

ADR 0027 stays outside the gate. The first replica may be raised while
layered data-path authorization is still Proposed.

## Consequences

- The tenant runtime remains at zero replicas until the JWT
  `SecurityPolicy` on `/v1/data` is attached and the rejections of unsigned
  and wrong-issuer tokens are recorded against the digest that served them.
- [#113](https://github.com/FieldstateNZ/saas-fabric/issues/113) waits on
  that evidence before an end-to-end tenant request can be served.
- ADR 0027 may be accepted, amended or left Proposed without moving this
  gate.
- This repository still writes no gateway resource, JWT policy or replica
  count. Those stay in `saas-fabric-platform`, behind the security gate
  ADR 0019 already requires.
