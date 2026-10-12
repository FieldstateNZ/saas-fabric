# ADR 0017 — Fabric decides which client secret boundary an operation reaches

- **Status:** Accepted; amended 2026-10-12 under the D01-25 disposition in
  [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)
  ([Versions, rollback and retention](#amendment-2026-10-12-d01-25-versions-rollback-and-retention))
- **Date:** 2026-08-31; amended 2026-10-12
- **Applies to:** `fabric-control-plane`, `fabric-openbao`, `fabric-client-model`, and the platform's OpenBao policy
- **Related:** [ADR 0008](0008-desired-state-is-the-authority.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md); [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md)

## Context

A client's secrets live in that client's OpenBao **namespace**. The namespace
is the boundary; there is no second partition abstraction layered on top of it.

The platform reaches those namespaces with one credential. Measured against a
real store while building this: policies are looked up in the *token's*
namespace, so a policy granting `secret/*` grants the root namespace and no
client's — every client operation answers `permission denied`. What works is a
single-segment glob:

```hcl
path "+/secret/*" { capabilities = ["create", "read", "update", "delete", "list"] }
```

`+` matches exactly one namespace segment, so this grants every client's
boundary and nothing nested inside one. It also never needs reconciling when a
client is added, which is why it is preferred to enumerating namespaces.

The consequence is the thing worth being explicit about: **the platform
credential is deliberately capable of reaching every first-level client
namespace.** It is not scoped to one client, and no request makes it so.

## Decision

**The Fabric control plane uses a platform OpenBao credential authorised for
first-level client namespaces. For every client secret operation, Fabric SHALL
authorise the operator for the selected client and derive the OpenBao namespace
exclusively from trusted client desired state. Callers SHALL NOT supply or
override namespace, mount, OpenBao address, token, or policy.**

```text
operator asks for Acme
        ↓
Fabric authorises the operator          ← a security boundary
        ↓
trusted desired state resolves Acme → namespace   ← a security boundary
        ↓
platform OpenBao credential
        ↓
+/secret/*
```

### The trust split, stated plainly

> **OpenBao enforces namespace isolation. Fabric enforces which namespace a
> given operator operation may target.**

Both halves are load-bearing and neither substitutes for the other. OpenBao
guarantees that a request carrying `X-Vault-Namespace: acme` cannot see
Contoso's secrets — measured: the same path answers `404` without the header
and in another namespace. It guarantees nothing about *which* namespace Fabric
puts in that header.

That choice is Fabric's, and it is therefore a security boundary rather than
convenience logic. Two properties make it one:

- the namespace is read from the client's desired state, which an operator
  changes through the control plane and Git records — there is no request field
  it can arrive in, and `SecretNamespace` has no deserialisation path from a
  request body
- a client with no declared boundary is **refused**, never defaulted to
  something derived from its id, because a derived boundary is another client's
  boundary the first time the derivation is wrong

### Why not a credential per client

It would narrow the blast radius of a bug in the resolution above, and it would
cost a credential lifecycle per client — issuing, rotating, revoking, and
storing — for a central control-plane component that already authenticates
every operator and already reads every client's desired state. The trust
boundary is clear and enforced in one place; per-client credentials are a
larger change for a marginal narrowing, and are not ruled out later.

## Consequences

**The resolution and the authorisation are the things to review.** A defect in
either reaches every client, which is the price of the broad credential. They
are small, in one place, and covered by tests that assert a traversal cannot
climb out of a boundary and that no secret route answers without an operator.

**A path is the only thing a caller supplies**, so it is validated as such:
absolute paths, empty segments, `.`, `..` and `%` are refused before anything
downstream sees them — the boundary is enforced by prefixing a namespace, and a
path that can climb out of its prefix makes that enforcement decorative.

**Reveal is recorded like any other operation.** Reading a secret is an act,
and the audit record carries actor, client, path, operation, version and
outcome — and can carry no value, because nothing that could contain one is
passed to the function that writes it.

> **Correction (2026-10-12, D01-25).** The record has a `version` field, and
> only a write fills it, with the version the write produced. A reveal or a
> delete records it empty: the port's `reveal` returns values and no version.

**The platform's policy is part of this decision.** `+/secret/*` belongs in
`saas-fabric-platform` beside the role that carries it, and narrowing it to
enumerated namespaces later is a policy change rather than a code change.

## Amendment (2026-10-12, D01-25): versions, rollback and retention

The decision above says where a secret boundary is and who may reach it. It
said nothing about a secret's versions. The D01-25 disposition in
[#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)
settles that: **roll forward under the existing check-and-set only; no
historical reveal, undelete or destroy endpoint; document retention
explicitly; propose its actual limits separately before changing a live mount
or shortening retained history.**

### 1. Rolling back is rolling forward

A secret is rolled back by writing the earlier values as a new version:

```text
version 2 is bad, version 1 was good
        ↓
PUT /api/clients/{clientId}/secrets/entry/{path}
    {"values": {…version 1's values…}, "expectedVersion": 2}
        ↓
version 3 holds version 1's values; versions 1 and 2 are unchanged
```

- **It is an ordinary write.** It names the version it replaces, and a
  mismatch is `409 secret_stale_version`, so two operators rolling back from
  the same view cannot both succeed. A write with no `expectedVersion` means
  "this does not exist yet" and is refused against an existing secret.
- **Version numbers only go up.** Nothing resets a version or restores one in
  place. The audit record names the version the write produced; the version it
  replaced is the one before it.
- **The earlier values come from outside Fabric.** Fabric reveals the current
  version only (section 2). Where the secret is a credential, issuing a fresh
  one at its source and writing that forward is preferred to restoring an
  older value, which is a credential that was already retired once.
- **The values are the whole secret.** A write replaces every key. The
  console's write form takes one key and one value, so rolling back a secret
  with several keys goes through the API with every key. Through the form, the
  keys it was not given would be dropped.

### 2. What Fabric does not offer

| Not offered | What exists instead |
|---|---|
| Reading an older version | Reveal reads the current version. No route, field or port parameter names a version |
| Restoring a version in place, or resetting the version number | A roll forward (section 1) |
| Undelete | None. Delete is not a rollback tool |
| Deleting or destroying one version | Delete removes the secret and every version, through the store's metadata endpoint, irreversibly. It is refused on a retired client (D01-14). A secret deleted and written again starts at version 1 |
| Changing how many versions are kept, or for how long | Nothing in Fabric. See section 3 |

Each of these is option B (version read and restore) or C (retention and
destroy) of D01-25. Adding one is a new decision and a new amendment, not a
patch. What holds the line:

- `scripts/check_architecture.py`, "Client secrets reach only the current
  version": `ClientSecrets` has exactly list, metadata, reveal, write and
  delete; reveal takes no version; the adapter names only the store's `data`
  and `metadata` endpoints, with no version selector and none of its history or
  retention vocabulary.
- `crates/fabric-control-plane/tests/client_secrets_boundary.rs`: a reveal or
  a write naming a version is refused, and no route undeletes, destroys or
  restores.
- `crates/fabric-openbao/tests/client_secrets.rs`, against a real store:
  rolling back writes version N+1 under check-and-set, a write naming no
  version never replaces, and a deleted secret leaves no version behind.

### 3. Retention, as it stands

Fabric sets no retention. Neither this repository nor `saas-fabric-platform`
(read at `main` `4152687`) sets `max_versions`, `delete_version_after` or
`cas_required` on any mount, so the store's defaults apply.

Measured on 2026-10-12 against OpenBao 2.7.1 started by
`scripts/secrets-store.sh`, on a client namespace's `secret/` mount created
the way that script creates it (`type: kv`, `version: 2`, no other options):

| Setting | Value read | What it means |
|---|---|---|
| `max_versions` | `0` | The store's default of 10 applies. After 12 writes, versions 3 to 12 are kept and version 1 answers `404` |
| `delete_version_after` | `0s` | No version expires by age |
| `cas_required` | `false` | The store does not demand check-and-set. Fabric sends `cas` on every write regardless |

What follows from that:

- **Up to nine earlier values of every secret stay in the store.** Fabric
  cannot read them, but anything holding `read` on the secret's data path can,
  with `?version=N`.
- **A roll forward does not remove a leaked value.** It stays readable in
  the store until nine later writes push it out. The remedy for a leaked
  credential is to revoke it at its issuer and write the replacement forward.
  Removing the stored copy is a destroy, which is not offered (option C).
- **The platform credential reaches history, even though Fabric does not.**
  Measured against the same store, the policy in this ADR (`+/secret/*` with
  create, read, update, delete and list) also permits reading an older
  version, soft-deleting, undeleting and destroying versions, writing the
  mount's `config` (its `max_versions`), and writing a secret's own metadata.
  Fabric's code calls none of these (section 2). It is the policy, not the
  code, that leaves them reachable.

**Not verified:**

- Where a client namespace's `secret/` mount is created on a live
  environment, and with what options. Platform `main` at `4152687` mounts only
  the root namespace's `secret/`, with no retention options, and says
  `secret/clients/*` is created by client provisioning. No client mount is
  declared in either repository.
- That a live environment carries `+/secret/*`. On platform `main` at
  `4152687`, LucentRoot's `saas-fabric-control-plane` policy grants only the
  instance partition (`secret/{data,metadata}/platform/saas-fabric/instances/master/*`).
  Read from Git and not observed live: if that is what runs, a client secret
  operation there is refused.

### 4. What this amendment does not change

No mount, policy, retention setting or retained version is changed by it. The
actual limits are proposed separately, on
[#108](https://github.com/FieldstateNZ/saas-fabric/issues/108), and wait for
a decision there before anything is applied. Shortening retained history is
irreversible for every version it drops, and a lower `max_versions` is a
shorter window to recover from, including under option B if it is taken
later.

