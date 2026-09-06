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
- `OidcClient { id: OidcClientId, protocol: ClientProtocol, pkce: PkceMethod, redirect: RedirectStrategy }`.
  `ClientProtocol::Oidc` only. Serialised with `type` for `protocol`.
  **No secret field, ever** — every declared client is reconciled as public.
  `pkce` is required with no default; `PkceMethod::S256` is the only variant
  and `as_wire_value()` is the one place `"S256"` is spelled.
- `RedirectStrategy` — a **struct with private fields** (`kind`, `uris`) plus
  `try_new(kind, uris) -> Result<_, DesiredStateError>`, `kind()` and `uris()`.
  A struct rather than a four-variant enum because the two fields only mean
  anything together and Rust cannot make an enum variant's field private.
  `RedirectStrategyKind::{ClaimedHttps, PrivateNetwork, Development, CustomScheme(AppScheme)}`
  is the closed four-way set ADR 0019 §3 states. Serde goes through
  `RedirectStrategyShape` in **both** directions — internally tagged on
  `strategy`, camelCase values, `deny_unknown_fields`.
- `RedirectUriKind { PrivateUseScheme, Loopback, PrivateNetwork, Https }` —
  what a strategy is stated against. Decided once, at `RedirectUri::try_new`,
  and carried on the value. `Https` is a **positive** rule
  (`identity/redirect_uri/host_kind/registered_domain.rs`): the host has to be
  a registered domain, not merely something no parser read as an address.
- `AppScheme` — a private-use URI scheme, RFC 8252 §7.1 reverse-domain form.
- `required_roles::{REQUIRED_ROLES, first_missing}` — `["Client Realm Administrator", "Client Realm User"]`.
- Names: `ClientId`, `RealmName`, `OidcClientId` (macro, `slug_newtype!`),
  `RoleName`, `Host`, `RedirectUri`, `ClientRevision` (hand-written rules).
  All validate on deserialisation via `#[serde(try_from = "String")]`.
- `DesiredStateError` — `Malformed{detail}`, `UnknownDocumentKind{expected, found}`,
  `MissingField{field}`, `InvalidField{field, detail}`, `RequiredRoleMissing{role}`,
  `Duplicate{field, value}`, `Deferred{field, phase, detail}`,
  `Migration{field, replacement, detail}`. Every variant is a *caller* problem;
  no I/O variant. The control plane maps the whole type — `400 invalid_request`
  on write, `500 desired_state_invalid` on read — so a new variant needs no new
  status mapping.
- `API_VERSION = "fabric.fieldstate.nz/v1"` (deprecated, still read),
  `API_VERSION_V2 = "fabric.fieldstate.nz/v2"` (written), `KIND = "Client"`.

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
7. **Classification is scheme first, then host, and lives in exactly one
   place** (`identity/redirect_uri/kind.rs`). A private-use scheme is one
   whatever its authority. A second copy of the partition is a second answer
   waiting to disagree.
8. **A private-use scheme must keep requiring a dot.** A branch admitting any
   `scheme:` that is not `http` would admit `javascript:` — the regression
   `refuses_a_javascript_scheme` is mutation-proved against.
9. **The parser widens universally; the strategy narrows.** A trailing `*` is
   a spelling `RedirectUri::try_new` accepts anywhere; which strategies may
   hold one is `redirect_strategy::rules`' question. Keeping the two apart is
   what stops the parser growing a second copy of the strategy table — and it
   is why a wildcard in the *host* is still refused, mutation-proved. A `*` in
   the **port** is refused outright: Keycloak matches nothing against `:*`, and
   over `http` a portless loopback callback already matches any port.
10. **`v1` keeps parsing, and the migrator stays total.** Every
    `RedirectUriKind` has an arm, including the private-use one that `v1`
    could not hold. A mixed list is refused, never resolved.

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
