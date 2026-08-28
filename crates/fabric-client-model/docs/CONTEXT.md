# fabric-client-model — LLM context

The control plane's desired-state model. Depends on `fabric-core`, `serde`,
`serde_norway`, `thiserror`. **No I/O, no HTTP, no Git, no Keycloak.**

## Public surface

- `Client { id: ClientId, display_name: String, hosts: Vec<Host>, identity: IdentityConfiguration }`
  — the *modelled view*. Complete enough to render a screen and reconcile
  identity from; **not** complete enough to serialise as desired state.
- `ClientDocument` — `parse(&str)`, `client() -> &Client`, `into_client()`,
  `with_identity(IdentityConfiguration) -> Result<Self, _>`, `render() -> Result<String, _>`.
  Holds the whole parsed YAML *and* the typed view. `with_identity` replaces
  only `spec.identity` and re-parses the merged result.
- `IdentityConfiguration { realm: RealmName, roles: Vec<RoleName>, clients: Vec<OidcClient> }`
  + `validate()`. Lists are `Vec` not `BTreeSet` **on purpose**: this is
  serialised back into a Git document a human reviews in a diff, and a set would
  reorder an operator's list on every write.
- `OidcClient { id: OidcClientId, protocol: ClientProtocol, redirect_uris: Vec<RedirectUri> }`.
  `ClientProtocol::Oidc` only. Serialised with `type` for `protocol`.
  **No secret field, ever** — every declared client is reconciled as public.
- `required_roles::{REQUIRED_ROLES, first_missing}` — `["Client Realm Administrator", "Client Realm User"]`.
- Names: `ClientId`, `RealmName`, `OidcClientId` (macro, `slug_newtype!`),
  `RoleName`, `Host`, `RedirectUri`, `ClientRevision` (hand-written rules).
  All validate on deserialisation via `#[serde(try_from = "String")]`.
- `DesiredStateError` — `Malformed{detail}`, `UnknownDocumentKind{expected, found}`,
  `MissingField{field}`, `InvalidField{field, detail}`, `RequiredRoleMissing{role}`,
  `Duplicate{field, value}`. Every variant is a *caller* problem; no I/O variant.
- `API_VERSION = "fabric.fieldstate.nz/v1"`, `KIND = "Client"`.

## Hard invariants — do not break

1. **`with_identity` must never round-trip through `Client`.** The raw document
   is the thing that gets written; the typed view is derived from it. Anything
   else deletes `spec.features`, `spec.data`, and every future section.
2. **Parse checks `apiVersion`/`kind` before deserialising the shape.** A
   `kind: Tenant` document must be refused as the wrong kind, not as an
   incomplete client.
3. **Required roles cannot be removed.** `validate` refuses it, and it is the
   one rule an operator can break from the UI.
4. **No secret may become expressible in this document.** Adding a
   `clientSecret` field to `OidcClient` would put credentials in Git.
5. **`RoleName` must keep refusing doubled and boundary whitespace.** The
   reconciler compares role names for equality against the provider.
6. **`ClientRevision` stays opaque.** No ordering, no parsing, no construction
   outside a repository adapter's response.

## Design notes

- `serde_norway` is the maintained fork of `serde_yaml`. Verified at 0.9.42:
  MIT OR Apache-2.0. YAML rather than JSON because the desired-state repository
  is edited by humans; it is confined to this crate.
- Comments do not survive a round trip. Accepted, and the reason the control
  plane rewrites only what an operator changed.
- `display_name` is free text rather than a newtype: it is displayed and never
  interpolated into a path, URL, or query. It is still bounded and refused if
  it contains control characters, because it reaches log lines and a commit
  message.
