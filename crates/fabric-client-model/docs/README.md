# fabric-client-model

The control plane's desired-state model: what a SaaS Fabric **client** is, and
the declarative document that carries it in Git.

No I/O. No Git, no Keycloak, no HTTP. This crate knows what a client should
look like and how to read and write the document that says so.

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
```

The full contract, including what the platform requires and what it refuses, is
[`docs/architecture/client-desired-state.md`](../../../docs/architecture/client-desired-state.md).

## Two schema versions, and the migrator between them

`v2` ships **beside** `v1`, which is the policy this crate already wrote down
(`src/document/schema.rs`) being exercised rather than amended. `v1` is
deprecated and still read.

`v2` says two things `v1` could not: `pkce: s256`, required with no default,
and a `redirect` block carrying the **strategy** — which kind of callback the
client is entitled to. A `v1` document's flat `redirectUris` list was
individually validated and still said nothing about what the client *was*, so a
production client could quietly hold a loopback callback and pass every check.

A `v1` document is read through one narrow migrator (`src/document/migration.rs`).
Every entry is classified, and the whole list must agree:

| All entries are | Read as |
|---|---|
| public `https://` | `claimedHttps` |
| `.internal` hosts | `privateNetwork` |
| loopback | `development` |
| a **mix**, or a private-use scheme | **refused** — migrate the document by hand |

The mix is refused rather than resolved because there is no honest resolution:
picking the looser strategy would silently grant an entitlement the operator
never stated.

**An edit migrates the document, in place.** `with_identity` rewrites
`apiVersion` to `v2` and writes the `v2` client shape. That is forced rather
than chosen: the edit re-parses what it rendered, and a `v2` identity block
under a `v1` `apiVersion` cannot survive that. A file nobody edits stays `v1` —
nothing reinterprets a document at rest.

## The one design decision worth knowing

**`ClientDocument` keeps the whole parsed document, not just the modelled
part.**

The obvious design parses into a `Client` and serialises back out of it. That
design silently deletes every section this model has no field for — so an
operator adding a realm role would also drop the client's feature flags, and
the only evidence would be in a Git diff nobody reads until something stops
working.

So `with_identity` replaces exactly the `spec.identity` sub-tree of the parsed
document, keeping every other key and value — and their order — then re-parses
the result so the two halves cannot disagree.

What it does *not* keep is formatting. This is a YAML data parser and the
writer reprints the file, so comments and blank lines are lost, quoting is
normalised, flow sequences become block sequences, and a folded scalar comes
back as a literal block holding the same string. Values survive; the shape of
the file does not.

That cost is why the control plane rewrites only documents an operator actually
changed, rather than normalising the repository on read. See
`docs/architecture/client-desired-state.md` for the worked example.

## Client and tenant

`ClientId` and `fabric_core::TenantId` hold the same string for the same
organisation — client `acme` is tenant `acme` is realm `acme`. They are
separate types because they belong to different planes and are established by
different means: a `TenantId` comes from a request's bearer token (§10), a
`ClientId` from a path an operator addressed. Sharing one type would make it
possible to hand a runtime tenant identity to a control-plane operation, and
neither direction should type-check.

Both validate with the same rule from `fabric_core::naming`, so they cannot
disagree about which strings are legal.

## Names, and why each has the rule it has

| Type | Rule | Because |
|---|---|---|
| `ClientId`, `RealmName` | strict DNS label | becomes a URL path segment, a directory name, and a realm name |
| `OidcClientId` | identifier (allows `_`) | written by a platform engineer, not derived from tenant input |
| `RoleName` | letters, digits, single interior spaces, `-_.` | a human phrase, compared against what the identity provider returns |
| `Host` | DNS labels separated by dots | no scheme, no port, no path — a host that carried `https://` would produce a route that never matches |
| `RedirectUri` | classified into one of four kinds, scheme first then host; one trailing `*`, and `*` as the whole port | the security boundary of an OAuth flow |
| `AppScheme` | RFC 8252 §7.1 reverse domain, lower-cased | a private-use scheme any other app on the device can also register |
| `ClientRevision` | opaque, entity-tag safe | compared for equality and nothing else |

`RoleName` refusing a doubled interior space is not fussiness. The reconciler
decides whether a role exists by comparing this value with what the provider
returned, so `Client  Realm User` would be created on every pass, forever, and
no operator reading either screen could see why.

`ClientRevision` is deliberately **not** shaped like `BindingRevision`. The
runtime plane's revision is a monotonic counter it can compare with `>`; this
one is a content hash, and giving it the same shape would invite code asking
whether one desired state is *newer* than another — a question a hash cannot
answer.

## Validation

`IdentityConfiguration::validate` runs at three points: when a stored document
is parsed, when an operator submits a change, and again after the change has
been merged into the document. The third looks redundant and is what makes "the
repository never holds a document this model would refuse to read" true by
construction rather than by review.

It enforces:

- roles are unique, and include every entry in `required_roles::REQUIRED_ROLES`;
- application clients are uniquely named;
- every application client declares at least one redirect URI;
- every redirect URI's **kind** is one its strategy admits — a URI outside the
  strategy is refused, never reclassified into a strategy that would take it;
- wildcards: a trailing path `*` and a wildcard port only under `development`,
  because RFC 9700 §2.1 requires exact matching everywhere else;
- `customScheme` is refused, naming `Lane E phase 2`. The shape is in the model
  so documents do not have to change again when it lands.

### The partition a strategy is stated against

**Scheme first, then host, both lower-cased.** A private-use scheme is a
private-use scheme whatever its authority, so
`nz.fieldstate.slipway://localhost/cb` is *not* a loopback callback — a
host-first rule would hand a native application's callback the entitlement a
development HTTP callback has. Within `http`/`https` the host decides:
`https://localhost:5173/cb` is **loopback**, not the production kind.

Loopback is `127.0.0.1`, `::1` and `localhost`, and nothing else. `127.0.0.2`,
`[::ffff:127.0.0.1]` and `localhost.localdomain` all reach loopback on some
machine and are all refused, with a message naming the boundary: an entitlement
that can only be recognised by resolving a name is not a declaration.
