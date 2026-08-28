# The client desired-state document

- **Status:** Implemented
- **Owned by:** [`fabric-client-model`](../../crates/fabric-client-model)
- **Stored in:** `saas-fabric-clients`, at `clients/<client id>/client.yaml`
- **Related:** [ADR 0008](../decisions/0008-desired-state-is-the-authority.md),
  the platform specification §4

This is the contract between SaaS Fabric and the repository that holds what each
client should have. It is the thing an operator's change becomes, and the thing
reconciliation reads.

## The document

```yaml
apiVersion: fabric.fieldstate.nz/v1
kind: Client

metadata:
  name: acme

spec:
  displayName: Acme

  hosts:
    - www.example.com

  identity:
    realm: acme

    roles:
      - Client Realm Administrator
      - Client Realm User

    clients:
      - id: web
        type: oidc
        redirectUris:
          - https://www.example.com/callback

  # Sections this model does not understand yet. They are preserved exactly.
  features:
    invoicing: true

  data:
    primary:
      class: dedicated
```

Working examples live in [`examples/clients`](../../examples/clients) and are
parsed by a test, so they cannot drift from the code.

## `kind: Client`, and its relationship to `kind: Tenant`

The platform specification §4 shows a tenant document. A **client** and a
**tenant** are the same organisation seen from two planes: client `acme` is
tenant `acme` is realm `acme`.

The control plane's documents are client-shaped because *client* is the word an
operator uses, and because `saas-fabric-clients` is where the platform already
said client definitions live. The two are not competing formats — this is the
one the control plane reads and writes, and it carries the same
`spec.identity.realm` and `spec.hosts` §4 describes.

`apiVersion` and `kind` are **checked before the rest is parsed**. A document
labelled something else is refused as the wrong kind, not as a mysteriously
incomplete client — the first message points at the actual problem, the second
sends someone looking for a field the document was never supposed to have.

A change to this format ships as `v2` alongside `v1`, never as a
reinterpretation of documents already in the repository.

## Fields

| Path | Required | Rule |
|---|---|---|
| `apiVersion` | yes | exactly `fabric.fieldstate.nz/v1` |
| `kind` | yes | exactly `Client` |
| `metadata.name` | yes | DNS label, ≤63 bytes — the client id, and the directory name |
| `spec.displayName` | yes | free text, ≤128 bytes, no control characters |
| `spec.hosts` | no | DNS hostnames: no scheme, no port, no path, no trailing dot |
| `spec.identity.realm` | yes | DNS label |
| `spec.identity.roles` | yes | unique; must include both required roles |
| `spec.identity.clients` | no | unique `id`s; each needs at least one redirect URI |
| `spec.identity.clients[].type` | yes | `oidc` |
| `spec.identity.clients[].redirectUris` | yes | `https://` anywhere, `http://` only on loopback, at most one trailing `*` |
| anything else under `spec` | — | preserved untouched |

Unknown fields **inside `spec.identity`** are refused. Unknown fields
**elsewhere under `spec`** are preserved. That asymmetry is deliberate: the
identity block is the part this model owns and must understand completely, while
the rest belongs to capabilities that do not exist yet.

### The required roles

```text
Client Realm Administrator
Client Realm User
```

Part of SaaS Fabric's contract with a client rather than a Keycloak convention:
the platform's authorisation model assumes a realm distinguishes an
administrator of the client from an ordinary user of it, and every client-facing
surface built on that assumption breaks if one is missing. Removing either is
refused, on read and on write.

### Why a role name may not contain a doubled space

`Client  Realm User` renders identically to `Client Realm User` and compares
unequal. Reconciliation decides whether a role exists by comparing this value
with what the identity provider returned, so the near-miss would be created on
every pass, forever, and no operator reading either screen could see why.

### Why redirect URIs are validated here

A redirect URI is the security boundary of an OAuth flow: an over-broad entry is
how an authorisation code ends up somewhere it should not. The identity provider
would accept almost anything, so the refusal has to happen **before** the value
is written to Git — otherwise the dangerous value is already the desired state
and the platform is arguing with its own source of truth.

A single trailing `*` is allowed because a callback path prefix is the ordinary
case. A `*` anywhere else is refused: a wildcard in the *host* is the mistake
this check exists to prevent.

## What may never appear

**No secrets.** Not a client secret, not a database password, not a token, not a
certificate. §4 of the platform specification says so, and the model has no
field that could hold one — every declared application client is reconciled as a
*public* client, precisely so that no confidential client's secret needs a home.

Supporting confidential clients means designing secret delivery first.

## Preservation

An identity edit through the control plane rewrites `spec.identity` and
preserves **every other key and value in the document, and their order**. That
is the reason `ClientDocument` keeps the whole parsed document rather than
round-tripping through a struct: a struct would silently drop every section it
has no field for, and the only evidence would be a Git diff nobody reads until
a feature flag stops working.

What it does **not** preserve is the document's *formatting*. The parser reads
YAML as data and the writer reprints the whole file, so a round trip normalises
it:

| Written by hand | Comes back as |
|---|---|
| `# a comment` | gone |
| a blank line between sections | gone |
| `displayName: "Acme"` | `displayName: Acme` |
| `hosts: [a.example.com, b.example.com]` | a block sequence, one host per line |
| `note: >` (folded) | `note: \|` (literal), with the same value |

The values survive all of that — a folded scalar and the literal block it
becomes hold the same string — but the file an operator wrote is not the file
that comes back.

This is the sharpest cost of the design, and it is why the control plane
rewrites **only documents an operator has actually changed** rather than
normalising the repository on read. One client's identity edit reformats one
client's file; it never touches the other ninety-nine.

If comment-preserving edits become a requirement, that is a change of parser
(a document model that keeps its own trivia) rather than a change of policy —
and it is worth knowing that before the repository fills with comments somebody
expects to survive.

## Revisions

The control plane identifies a version of a document by an opaque **revision** —
the stored file's blob hash, in the Git-backed implementation. Nothing above the
repository parses, orders, or constructs one; it is compared for equality, which
is what optimistic concurrency needs and all it needs.

Deliberately not shaped like the runtime plane's `BindingRevision`, which is a
counter and can be compared with `>`. A content hash cannot answer "is this
newer?", and a type that invited the question would produce a bug that only
appears under concurrent edits.
