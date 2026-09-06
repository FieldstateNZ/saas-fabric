# The client desired-state document

- **Status:** Implemented
- **Owned by:** [`fabric-client-model`](../../crates/fabric-client-model)
- **Stored in:** `saas-fabric-clients`, at `clients/<client id>/client.yaml`
- **Related:** [ADR 0008](../decisions/0008-desired-state-is-the-authority.md),
  [ADR 0019](../decisions/0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md),
  [the identity edge test matrix](identity-edge-test-matrix.md),
  the platform specification §4

This is the contract between SaaS Fabric and the repository that holds what each
client should have. It is the thing an operator's change becomes, and the thing
reconciliation reads.

## The document

```yaml
apiVersion: fabric.fieldstate.nz/v2
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
        pkce: s256
        redirect:
          strategy: claimedHttps
          uris:
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

## Versions: `v2` beside `v1`

`fabric.fieldstate.nz/v2` is the current version. `v1` is **deprecated and
still read**.

The sentence directly above this section is not a policy being announced here —
it is the policy this page already carried, now being **exercised for the first
time**. The same rule is stated in the code that enforces it
(`crates/fabric-client-model/src/document/schema.rs:6-11`), and
[ADR 0019](../decisions/0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md)
§5 quotes both and amends neither. `v2` was added beside `v1`; nothing about a
document already in a repository changed meaning.

### What `v2` changed

| | `v1` | `v2` |
|---|---|---|
| PKCE | not expressible | `pkce: s256`, **required**. No default |
| Callbacks | `redirectUris`, a flat list | `redirect`, carrying a `strategy` and its `uris` |

The strategy is what a `v1` document could not say: **which kind of callback
this client is entitled to.** A production client and a development client were
previously indistinguishable, and either could quietly hold the other's URI.

### How a `v1` client is read

A `v1` document keeps parsing. Each client's `redirectUris` entries are
classified, and the whole list must agree:

| All entries are | Read as |
|---|---|
| public `https://` | `strategy: claimedHttps` |
| `.internal` hosts | `strategy: privateNetwork` |
| loopback | `strategy: development` |
| a **mix** of those | **refused** — migrate the document by hand |
| a private-use scheme | **refused** — migrate the document by hand |

The mixed case is refused rather than resolved because there is no honest
resolution: a client holding both a production callback and a loopback one is
the exact ambiguity the strategy exists to remove, and picking the looser one
would silently grant an entitlement the operator never stated.

### An edit migrates the document, in place

**Editing a `v1` client's identity through the control plane returns a `v2`
document**, with `apiVersion` rewritten and the client written in the `v2`
shape. Every other section, and the order of every key, is preserved as
described under [Preservation](#preservation) — `apiVersion` keeps its position
too.

This is not a choice the implementation made. An edit merges the new
`spec.identity` in and then re-parses the whole rendered document, so that a
document handed to the repository has been read by exactly the code that will
read it back; a `v2` `identity` block under a `v1` `apiVersion` fails that
re-parse. The edit therefore migrates the document or it fails.

Two consequences worth stating plainly:

- **A document nobody edits stays `v1`, and stays labelled `v1`.** No sweep
  migrates anything, because the control plane rewrites only documents an
  operator actually changed. A repository will hold both versions for as long
  as some clients go unedited.
- **The console shows each document's version**, and says that an edit will
  migrate it. Nobody should meet the version change first in a review diff.

Separately from the document: the **next reconciliation sweep writes S256 to
every declared client in the identity provider, `v1` and `v2` alike.** That is
a deliberate runtime break for any public client not already performing PKCE,
and it needs notice to client teams before the sweep rather than after.

## Fields

`v2`. The rows marked are the ones that differ between the versions;
everything else is the same in both.

| Path | Required | Rule |
|---|---|---|
| `apiVersion` | yes | exactly `fabric.fieldstate.nz/v2`, or `fabric.fieldstate.nz/v1` (deprecated) |
| `kind` | yes | exactly `Client` |
| `metadata.name` | yes | DNS label, ≤63 bytes — the client id, and the directory name |
| `spec.displayName` | yes | free text, ≤128 bytes, no control characters |
| `spec.hosts` | no | DNS hostnames: no scheme, no port, no path, no trailing dot |
| `spec.identity.realm` | yes | DNS label |
| `spec.identity.roles` | yes | unique; must include both required roles |
| `spec.identity.clients` | no | unique `id`s; each needs at least one redirect URI |
| `spec.identity.clients[].type` | yes | `oidc` |
| `spec.identity.clients[].pkce` | yes (**`v2` only**) | exactly `s256`. No default — the document says it |
| `spec.identity.clients[].redirect.strategy` | yes (**`v2` only**) | one of `claimedHttps`, `privateNetwork`, `development`, `customScheme` |
| `spec.identity.clients[].redirect.uris` | yes (**`v2` only**) | non-empty; every entry's kind must be admitted by the strategy — see below |
| `spec.identity.clients[].redirectUris` | yes (**`v1` only**) | `https://` anywhere; `http://` only on loopback or under `.internal`; at most one trailing `*`. Replaced by `redirect` in `v2`, and refused in a `v2` document |
| anything else under `spec` | — | preserved untouched |

### What each redirect strategy admits

| Strategy | Admits | Wildcards |
|---|---|---|
| `claimedHttps` | public `https://` hosts only. The production rule, and what an iOS Universal Link or an Android App Link is | none |
| `privateNetwork` | `.internal` hosts, over `http` or `https`. LucentRoot's production posture | none |
| `development` | loopback — `127.0.0.1`, `::1`, `localhost` — over `http` or `https`, on **any port** | one trailing path `*`, and `*` in the port position |
| `customScheme` | a private-use URI scheme in reverse-domain form | none |

**Representable, and refused until Lane E phase 2:** `customScheme`. The shape
is in the model so that documents do not have to change again when it lands,
and a document declaring one is refused with a message naming the phase. It is
never coerced into another strategy.

**A URI is classified by its scheme first, then by its host.** A private-use
scheme is a private-use scheme whatever its authority, so
`nz.fieldstate.slipway://localhost/cb` is *not* a loopback callback. Within
`http` and `https` the host decides: `https://localhost:5173/cb` is
**loopback**, not a claimed-HTTPS callback, and `https://admin.corp.internal/cb`
is a **private-network** host. Scheme and host are compared case-insensitively.

Loopback means those three host spellings and no others. `127.0.0.2`,
`[::ffff:127.0.0.1]` and `localhost.localdomain` all reach loopback on some
machine and are all refused here, because an entitlement that can only be
recognised by resolving a name is not a declaration.

**A URI whose kind the strategy does not admit is refused**, naming the
strategy, the URI's kind, and what the strategy admits — never reclassified
into a strategy that would accept it.

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
this check exists to prevent. In `v2` a `*` is additionally allowed in the
**port** position, and only under `development`, because RFC 8252 §7.3 requires
a loopback redirect to work on whichever ephemeral port a native application
binds.

Validating each URI is necessary and was never sufficient. Every URI in a `v1`
document is individually acceptable and the *set* still says nothing about what
the client is: a production client holding a loopback callback passes every
check above. `v2`'s `redirect.strategy` is the missing half — the document
states the entitlement, and a URI outside it is refused rather than accepted
because it happens to parse.

### Why plain HTTP is permitted at all

`https://` is the rule. The exceptions are the two cases where requiring TLS
would require a certificate that **cannot exist**:

- **Loopback**, where the code never leaves the machine. This is what RFC 8252
  recommends for native applications, for the same reason.
- **The `.internal` top-level domain.** ICANN resolved in July 2024 to withhold
  it from delegation permanently, reserving it for private-use applications.
  Because it will never exist in the public DNS root it cannot resolve on the
  internet, and no public certificate authority will issue for it — so an
  internal environment reached over plain HTTP is not a deployment that should
  have TLS and skipped it.

LucentRoot is the second case: its gateway has one listener, on port 80, and
its hosts are `*.lucentroot.internal`.

The check examines the **authority only** — a `.internal` anywhere else in the
URI is not the question, and `http://evil.example.com/.internal` is refused.
Userinfo is refused outright rather than parsed around, because
`http://x.internal@evil.example.com/` is a public host wearing an
internal-looking prefix.

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
