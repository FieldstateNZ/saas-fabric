# fabric-reconciliation — LLM context

Converges an identity provider onto a client's desired state. Depends on
`fabric-core`, `fabric-client-model`, `async-trait`, `serde`, `thiserror`,
`tracing`. **No HTTP, no provider protocol.** Event domain `11`.

## Public surface

- `IdentityProvider` (async trait, `Arc<dyn>`) — `observe_realm(&RealmName) -> Option<ObservedRealm>`,
  `create_realm`, `set_realm_display_name`, `create_realm_role`,
  `create_oidc_client`, `update_oidc_client(realm, client, &HeldEdits)`,
  `configured_audience() -> Option<&str>`, `describe() -> String`. **Every
  create must be idempotent.** **There is no delete.** `configured_audience`
  returning `None` is not "no audience configured yet" in the ordinary sense —
  `IdentityReconciler::plan` treats it as a reason to refuse outright (see
  `ProviderError::NoAudienceConfigured` below).
- `ObservedRealm { display_name: String, roles: BTreeSet<RoleName>, clients: BTreeMap<OidcClientId, ObservedOidcClient> }`
  — only what the platform declares. Fields SaaS Fabric says nothing about are
  absent so the reconciler cannot notice, and therefore cannot overwrite, an
  operator's deliberate change to one.
- `ObservedOidcClient { redirect_uris: BTreeSet<RedirectUri>, public: bool, challenge_method: Option<PkceMethod>, audience_mapper: Option<String>, other_protocol_mappers: usize, unmodellable_redirect_uris: usize, enabled: bool, standard_flow_enabled: bool, post_logout_redirect_uris_is_every_registered_uri: bool }`.
  `challenge_method` is `None` for both "no such attribute" and "a value this
  model cannot read" — no `Plain` variant exists to hold the latter (ADR 0019
  §6). `audience_mapper` is the audience the client's mapper currently
  asserts, or `None` if it has none **or more than one** — see its own
  rustdoc. `other_protocol_mappers` is a **count** of the client's mappers
  that are not that one — a mapper nobody declared, added out of band, is
  corrected the same way a missing audience mapper is. `unmodellable_redirect_uris`
  is a **count**, never the values — they are attacker-influenced text with no
  reason to reach a plan, a log line, or an API response — and non-zero is
  drift regardless of what `redirect_uris` holds. `enabled`,
  `standard_flow_enabled`, and `post_logout_redirect_uris_is_every_registered_uri`
  close the gap where a declaration always writes `true` (or the literal `+`)
  but nothing read it back — a client disabled, or narrowed, by hand used to
  read as converged forever.
- `ProviderError` — `Unavailable{detail}`, `NotPermitted`, `Rejected{detail}`,
  `NoAudienceConfigured`. `is_transient()`. **`detail` must never carry an
  upstream response body or a credential** — the adapter is where that is
  dropped.
- `IdentityPlan` — `realm()`, `actions() -> &[IdentityAction]`, `hand_edits() -> &[HandEdit]`,
  `is_converged()`. A plan can be converged and still carry hand edits.
- `IdentityAction` — `CreateRealm{display_name}`, `SetRealmDisplayName{display_name}`,
  `CreateRealmRole(RoleName)`, `CreateOidcClient(OidcClient)`,
  `UpdateOidcClient { client: OidcClient, held: HeldEdits }`.
  Ordered: realm first, then roles, then clients.
- `HandEdit { client: OidcClientId, field: HandEditedField }` (`Display` as
  `client:field`), `HandEditedField` — `ClientDisabled | StandardFlowDisabled |
  DeclaredRedirectUriRemoved` (serialised camelCase), `HeldEdits { client_disabled,
  standard_flow_disabled, removed_redirect_uris }` with `is_empty()`, `fields()`,
  `redirect_uris_to_write(&[RedirectUri])`. D01-19b; see the README's
  "What the comparison sorts a difference into".
- `IdentityReconciler::new(Arc<dyn IdentityProvider>)` —
  `plan(&Client) -> Result<IdentityPlan, ProviderError>` (observe only, holding),
  `reconcile(&Client, &[OidcClientId]) -> ReconciliationOutcome` (observe, plan,
  apply; holds unsupported hand edits; leaves the named declared clients out), `reassert(&Client) -> ReconciliationOutcome` (the same,
  overwriting them — only an operator's audited request calls this).
- `ReconciliationOutcome` — `status()` (only `Applied` or `Failed`), `actions()`,
  `detail()`, `changed_nothing()`, `hand_edits()` (held), `withheld()`, `overwritten()`
  (re-assert only). Constructors `converged()`, `applied(n)`,
  `failed(&ProviderError)`, and builders `holding(..)`, `withholding(..)`, `overwriting(..)`, are
  `pub` so other crates' tests can build one; there is deliberately no
  constructor for `Pending`, `Drifted`, `HandEdited` or `Withheld`.
- `ReconciliationStatus` — `Pending | Applied | Failed | Drifted | HandEdited | Withheld`. `as_str()`,
  `Display`, `Serialize` (camelCase: `handEdited`; the rest single words).
- `ReconciliationReport { status, revision: ClientRevision, actions, observed_at_unix, detail, hand_edits, withheld }`.
- `ReconciliationStatusStore` — `report(&ClientId)`, `mark_pending(...)`,
  `record(...) -> ReconciliationStatus`. In-memory, `std::sync::Mutex` with
  poison recovery (no `unwrap`).
- `testing::FakeIdentityProvider` — `new()`, `with_audience(impl Into<String>)`,
  `fail_with`, `recover`, `calls`, `clear_calls`, `seed_realm`, `realm`. Used
  cross-crate by `fabric-control-plane`'s own tests, not as a development
  adapter — see its own module doc for why that claim was wrong.

## Hard invariants — do not break

1. **`reconcile` returns an outcome, not a `Result`.** A failed reconciliation
   is a fact about a client that has to be recorded and shown, not an error a
   caller may `?` away.
2. **`plan` returns `Result`.** An empty plan means "already converged", so a
   failed observation must never produce one.
3. **Nothing here deletes.** Adding a delete operation to the port needs a
   decision, not a commit.
4. **`apply` stops at the first failure.** Actions depend on each other; every
   one is additive and idempotent, so the next pass continues from what exists.
5. **The diff compares role *containment*, not set equality.** The provider
   creates roles of its own; equality would try to "correct" every realm
   forever.
6. **Redirect URIs compare as sets.** Order from a provider is arbitrary, so
   reordering is not a difference — but a legitimate extra URI the provider
   holds beyond what is declared is drift too, exactly as an unparseable one
   is (ADR 0019 §6, D13b). A declared URI that is missing is a held hand
   edit (D01-19b), not drift.
7. **`Drifted` and `HandEdited` are decided by `status::transition`, from the
   previous report and the outcome's held edits.** Not in two places.
8. **The audience is a provider fact, not a document field.** A client
   document has no way to name its own audience — see the design note below —
   so the comparison never reads one off `OidcClient`.
9. **A provider that names no audience is refused, not planned against.**
   `IdentityReconciler::plan` returns `ProviderError::NoAudienceConfigured`
   rather than comparing against an empty string or skipping the audience
   term — see `IdentityProvider::configured_audience`'s own rustdoc.
10. **A held hand edit is never written by an ordinary pass** (D01-19b), and
    a write that corrects product-owned drift beside one carries it as the
    provider holds it. Only `reassert` overwrites.
11. **A withheld client is left alone** (D01-19b). `reconcile(&Client,
    withheld)` leaves those declared clients out of the pass: never created,
    written or corrected. The outcome names those it actually left out, and
    the status store calls the pass `Withheld`, after `Failed` and before
    `HandEdited`. The control plane withholds a stored projected entry that
    is not its projection.

## Design notes

- The reconciler takes no clock. The caller stamps `observed_at_unix`, so the
  store and the audit record cannot disagree about when something happened.
- A declared client is always public, so an observed client held as confidential
  does not match and is corrected. That is the one property that changes what a
  client *is*.
- **The comparison** (`plan::classify::compare`) returns `drift` (product-owned:
  `!public`, `unmodellable_redirect_uris != 0`, an undeclared redirect URI,
  `challenge_method != Some(declared.pkce)`, `audience_mapper !=
  Some(configured_audience)`, `other_protocol_mappers != 0`,
  `!post_logout_redirect_uris_is_every_registered_uri`) and `held`
  (`!enabled`, `!standard_flow_enabled`, declared URIs missing). Under
  `HandEditPolicy::Hold` only `drift` writes, carrying `held`; under
  `Overwrite` either writes, holding nothing.
- **`HandEdited` is decided by `status::transition`**, before "changed
  nothing": a pass that held a hand edit is never `Applied`. A previous
  `HandEdited` at the same revision counts as converged for `Drifted`.
- **Where the audience comes from, and why.** ADR 0019 §1/§G5: one audience
  string per deployment, equal to the Data API's own required `aud`. It is
  deployment configuration of whichever adapter is doing the writing (the
  Keycloak adapter already holds it as `KeycloakConfig::audience`), not
  something a client document says — a document that could name its own
  audience could opt out of the edge's check. Two ways to get that string into
  this crate's comparison were considered: have the provider report it
  (`IdentityProvider::configured_audience`), or have `IdentityReconciler` take
  it as a second constructor argument threaded from the same configuration.
  This crate takes the **first**: `IdentityProvider` gained
  `configured_audience(&self) -> Option<&str>`, `IdentityReconciler::plan`
  reads it from the provider it already holds and passes it to `plan::plan`,
  and no change was needed to how a reconciler is constructed anywhere that
  builds one. That keeps a single source of truth for the string — the
  adapter that writes the mapper is the same object asked what it wrote —
  without adding a second, parallel configuration path into the control
  plane's composition root, and without this crate learning that a "Keycloak"
  exists: the trait method is phrased the same way `describe()` already is,
  as a fact a provider states about itself. The `Option` exists so a provider
  that cannot yet say what it writes can say so honestly, and `plan` refuses
  outright on `None` rather than comparing against an invented value.
