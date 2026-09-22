# ADR 0025 — Realm bootstrap is platform composition, done with the credential the platform generates

- **Status:** Accepted (2026-09-22, product owner)
- **Date:** 2026-09-22
- **Applies to:** `saas-fabric-platform` (a `master-instance` convergence and
  its generated credential), the master realm's instance resources, the
  operator sign-in of [ADR 0024](0024-an-instance-is-a-realm-signed-in-at-the-gateway-and-surfaces-are-federated-modules.md)
  slice 1, and the scope of [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md)
- **Related:** [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md);
  [ADR 0024](0024-an-instance-is-a-realm-signed-in-at-the-gateway-and-surfaces-are-federated-modules.md);
  the platform's own rule for OpenBao ("LucentRoot's OpenBao must need no
  human in its lifecycle", `scripts/check.py`); Karo's client directory,
  where `master/` is provisioned like every other client

## Context

The control-plane instance signs in at the gateway (ADR 0024). The gateway
needs a confidential client in the master realm, with a secret it can present
when it redeems a code; operators need the `fabric-operator` role and, for
ADR 0012's realm creation *as the operator*, master-realm `admin`. Until now
every one of those was on a list titled "hand-made prerequisites nothing
creates" — the console's client, the role, the operator grant — and the first
plan for the gateway added one more: a person creating `saas-fabric-gateway`
in Keycloak and writing its secret into OpenBao.

The product owner's rule, stated the day that plan was written: **nothing is
touched by hand.** Karo was the hand-managed version SaaS Fabric learns from;
what Karo does with files and OpenTofu — its `master/` directory is provisioned
like any client's — SaaS Fabric must do without a person.

ADR 0012 rules out the obvious automation. It decided that the platform holds
no authority over Keycloak and acts as the operator who asked, with their
bearer, because changing an organisation's identity provider ought to trace to
somebody who chose it. A gateway client is different in kind: nobody asks for
it, and it must exist before any operator can sign in through the gateway at
all. Having the control plane create it would need either a signed-in
operator to bootstrap — circular for a fresh environment, and dependent on the
console's own sign-in that ADR 0024 retires — or a credential of the control
plane's own, which reverses ADR 0012.

There is a third authority already in the cluster, and it is the platform's:
the Keycloak bootstrap administrator, whose password the platform generates
in-cluster and never types (`keycloak-credentials`, an External Secrets
generator; "configuration expected from outside this repository: nothing,
that is the point"). The platform *made* the realm's administrator. Composing
the realm's edge is the same kind of act as composing the Role the publisher
holds or the ConfigMaps the runtime mounts.

## Decision

### 1. The master realm's instance resources are provisioned by the platform

A platform Application, `master-instance`, converges the master realm's
instance resources every time it syncs: the `fabric-operator` role; the
gateway's confidential client `saas-fabric-gateway` with its redirect and
post-logout URIs derived from the instance's public origin; the console's
public client for as long as the console's own flow exists; and, for each
operator the environment declares, the `fabric-operator` role and master-realm
`admin`. It does so with the bootstrap administrator credential the platform
generated, through OpenTofu and the Keycloak provider — the same mechanism
Karo uses and the same templates the local `hosting/` harness already
carries — run as an Argo CD sync hook whose apply is idempotent and whose
drift check is the proof.

### 2. The gateway's secret is generated in-cluster and set on the client

`master-instance-credential` generates the client secret the way the OpenBao
seal key is generated: in-cluster, once, never refreshed. The convergence sets
that value on the Keycloak client; the OIDC `SecurityPolicy` reads the same
Kubernetes Secret. One object, three readers, no OpenBao path, no person.
Rotation is a convergence re-run against a regenerated value, not a refresh.

### 3. Operators are declared, not clicked

The environment's configuration names its operators. The convergence assigns
them the two roles. Who may operate the platform is a line in a file the
platform reads, and the last per-person act in the realm goes with it.

### 4. ADR 0012 is scoped, not reversed

ADR 0012 governs the *product*: the control plane still holds no Keycloak
credential and still changes client realms as the operator who asked. What
this decision adds is that **realm bootstrap** — the resources the instance
needs before any operator exists in it — is *platform composition*, performed
by the platform with the administrator it created, exactly as it composes
every other piece of the edge. The boundary between the two is the master
realm's instance resources: the platform owns those; the product owns what
operators do with them.

### 5. The rule is enforced, not remembered

`scripts/check.py` gains the invariant it already holds for OpenBao: for
LucentRoot the render must contain the credential generator and the
convergence, and must not contain the hand path it replaces. A plan that ends
in a person creating something in Keycloak fails the platform's own check.

## Consequences

### Good

- A fresh environment comes up with its sign-in complete: Keycloak, its
  administrator, the realm's instance resources, the gateway's policy — all
  from the repository, in wave order.
- Issue #70's "hand-made prerequisites nothing creates" list empties by
  construction rather than by effort.
- The first sign-in is the test, as it is for Karo; verification steps become
  diagnostics.

### Bad, and accepted

- The platform now holds and uses the bootstrap administrator's credential in
  a Job. It already generated it; the Job's ServiceAccount is scoped to that
  one Secret and its own state, and the Job runs only what the repository
  declares.
- OpenTofu state for the master instance lives in the cluster. Losing it
  does not lose the resources, but the next apply would try to create what
  already exists and be refused; recovery is an adoption — the same
  `adopt_existing` declaration LucentRoot uses for the two objects that
  predate the convergence — not a re-creation. `prevent_destroy` guards
  against a config typo deleting a client; it says nothing about state.
- Two more Applications and the repository's first `Job`. The pattern is the
  price of the rule.

## Alternatives rejected

- **Borrowed operator authority** (ADR 0012's posture, extended). Needs a
  human sign-in to bootstrap, through the console flow ADR 0024 retires.
- **A control-plane machine identity on the master realm.** Reverses ADR
  0012 and puts realm-admin authority in the product.
- **A person, once.** The rule.

## What this does not decide

Client-realm provisioning stays with the control plane under ADR 0012 for
now; whether client *instances* (ADR 0024 slice 5) are provisioned by the
platform the same way is the next question, and Karo's answer — every
directory alike — is the expected one. Local parity in `hosting/` (the
harness provisioning its own master instance and running the OAuth2 filter)
is ADR 0024 slice 1b.
