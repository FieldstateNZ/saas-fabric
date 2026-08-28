# fabric-control-plane

The operator-facing control plane: where a human expresses what a client should
have.

```text
operator                     a human, on the operator network
    ↓
control-plane UI             apps/control-plane-ui
    ↓
Control Plane API            this crate
    ↓
ClientRepository             this crate's port
    ↓
Git desired state            fabric-client-git → saas-fabric-clients
    ↓
reconciliation               fabric-reconciliation
    ↓
Keycloak                     fabric-keycloak
```

## The rule this crate exists to enforce

**An operator mutation writes desired state. It does not call a platform
service.**

There is no code path from a handler to Keycloak. `ControlPlaneState` holds two
things — the domain service and the operator authenticator — and the absence of
a third is the design: no handler can reach an identity provider, and none can
bypass the service to write a document directly.

The visible consequence: a successful `PUT` answers `pending`, not `applied`.
Writing the document and converging the provider are different events that fail
independently, and an API that reported them as one would be lying about the
second one every time.

## The API

```text
GET /api/clients                       list clients
GET /api/clients/{clientId}            one client's overview
GET /api/clients/{clientId}/identity   its identity, and reconciliation state
PUT /api/clients/{clientId}/identity   replace its identity  (If-Match required)
```

Note what is not there: nothing that names a file, nothing that edits a document
as text, and nothing that reaches an identity provider (§8).

`/api` is not versioned, where the Data API's prefix is `/v1/data`. The Data API
is consumed by applications the platform does not own, so a breaking change
ships as a second path served alongside the first. This API is consumed by
exactly one client — the operator console in this repository — and the two are
built and deployed together. That reasoning stops holding the moment anything
else calls it, and the answer then is the Data API's: mount `/api/v1` alongside
`/api` rather than changing what `/api` means.

## Optimistic concurrency

A write states the revision it is editing, in `If-Match`. What is refused, and
why each case matters:

| Sent | Answer | Because |
|---|---|---|
| nothing | `428` | a write with no expectation is last-writer-wins |
| `*` | `428` | means "if it exists", which is not a revision — a one-character opt-out |
| `W/"…"` | `428` | weak comparison permits equivalent-but-different, which here is a lost update |
| two tags | `428` | there is exactly one revision a caller can have read |
| a stale revision | `409` | somebody else got there first; re-read and redo |

The revision check happens **before** the no-op short-circuit, and the order is
load-bearing: a stale `If-Match` is refused even when the change would have been
a no-op, because letting an identical body through would make the precondition
mean "unless it does not matter".

The repository's own check is still the authoritative one — it is atomic, and
it closes the window between this crate's read and its write.

## Two identities that must never meet

The runtime plane resolves a **tenant** from a bearer token and serves that
tenant's data. This crate authenticates a **platform operator** and lets them
administer any client. Nothing here reads a `tenant_id`, and no operator
authority is derived from one. See [ADR 0009](../../../docs/decisions/0009-operator-identity-is-not-tenant-identity.md).

`Operator` is an axum extractor, so a handler that takes one cannot run without
one — the same device the runtime plane uses for `TenantIdentity`. Every handler
takes one, including the reads.

## Errors

Nine variants, each with its own machine code, because an operator needs to tell
them apart. Two things they never say:

1. **Nothing an upstream system said verbatim.** A Keycloak admin error body or
   a Git provider's JSON is replaced with a Fabric error (§23).
2. **Nothing about the repository's internals.** Not a path, not a branch, not
   a file (§8). "The client changed since you read it" is the operator's
   problem; which blob moved is not.

There is no "reconciliation pending" *error*, and §23 asks only that it be
distinguishable — which it is, as a status on a successful response. Reporting
a good write as an error because a downstream convergence has not happened yet
would make the normal path look broken.

## What is deliberately absent

- **Client creation.** `POST /api/clients` is not implemented: creating a client
  is a workflow — routing, data placement, secrets, a database — and this
  increment is the identity slice of it.
- **Deletion of anything.** Neither documents nor realm content.
- **Raw document editing.** The API exposes realms, roles and application
  clients. It never exposes a file path, a line number, or YAML.
