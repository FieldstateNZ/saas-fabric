# fabric-control-plane-api

The control plane's composition root, and its binary.

```bash
cargo run -p fabric-control-plane-api -- examples/control-plane.toml
```

That command needs no cluster, no Keycloak, and no GitHub token: the example
configuration runs with development adapters, and says so at startup.

## A second binary, not a second API on the first one

`fabric-api` and this process are separate images and separate deployments. Not
tidiness:

- **They face different networks.** The runtime API is on the product edge,
  reachable by every tenant's application. This one is on the operator plane and
  must not be reachable from the product edge at all.
- **They fail differently.** A control plane that cannot reach Git is broken; a
  runtime plane in the same situation has not noticed, because it never reads
  Git (§6). Sharing a process would couple their availability, which is the
  exact coupling the architecture forbids.
- **They authenticate different things** — a tenant, and a platform operator
  ([ADR 0009](../../../docs/decisions/0009-operator-identity-is-not-tenant-identity.md)).

## The graph

`startup::application::build` is the whole of it, and the order rules something
out at each step:

1. The desired-state repository, because everything else is about it.
2. The identity provider, and the reconciler over it.
3. The API, which is given the repository and the reconciliation status — and
   **not** the provider. There is no wiring here by which a handler could reach
   Keycloak, which is the structural form of [ADR 0008](../../../docs/decisions/0008-desired-state-is-the-authority.md).
4. The reconciliation loop, which is the only thing holding both.

## Configuration

No `Default`, and three of the four fields have no safe one:

- the **operator posture** has none — every possible one either locks the
  platform out or lets everybody in;
- the **desired-state repository** has none, because a default that quietly
  became an empty in-memory store would present a platform with clients as a
  platform with none;
- the **identity provider** has none, because a default that reconciled nothing
  would show every client as converged.

So a deployment states all three, and a missing section is a startup failure
rather than an inherited guess. Both adapter choices are tagged enums for the
same reason: a boolean or an optional section would let a deployment fall into a
development adapter by omission.

Settings are namespaced `FABRIC_CP_SETTING_`, with `__` for nesting. That is
narrower than the runtime host's `FABRIC_SETTING_` because both processes run in
one Kubernetes namespace, and a shared prefix would let one process's override
land in the other's configuration — where `deny_unknown_fields` would abort
startup for a reason nobody could see.

## Secrets

`secrets::resolve` reads the process environment: `keycloak/saas-fabric` becomes
`FABRIC_SECRET_KEYCLOAK_SAAS_FABRIC`, the same convention the runtime host uses.

That covers every delivery mechanism the platform actually uses — the External
Secrets Operator and the OpenBao agent both project values into a pod's
environment — but it is not a client for a secret store, and it is not meant to
be. The application's side of the contract is only that a value called something
arrives; how it arrives belongs to `saas-fabric-platform` (§20, §21).

A missing secret is a startup failure. A control plane that cannot authenticate
to Git or Keycloak can do nothing useful, and discovering that at startup beats
discovering it on an operator's first write.

## `/health`, and the readiness probe that is deliberately absent

`/health` is the one route that does not require an operator: a kubelet has no
operator identity and cannot be given one. It is safe because it says nothing —
no client, no configuration, no status.

There is no `/ready`. Readiness would have to answer "can this process reach Git
and Keycloak?", and answering it honestly means calling both on every probe —
putting probe traffic on the platform's dependencies and coupling the
deployment's health to theirs. The per-client reconciliation status the API
already exposes is the better signal for that question.
