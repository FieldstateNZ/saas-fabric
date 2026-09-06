# fabric-reconciliation — LLM context

Converges an identity provider onto a client's desired state. Depends on
`fabric-core`, `fabric-client-model`, `async-trait`, `serde`, `thiserror`,
`tracing`. **No HTTP, no provider protocol.** Event domain `11`.

## Public surface

- `IdentityProvider` (async trait, `Arc<dyn>`) — `observe_realm(&RealmName) -> Option<ObservedRealm>`,
  `create_realm`, `set_realm_display_name`, `create_realm_role`,
  `create_oidc_client`, `update_oidc_client`, `configured_audience() -> &str`,
  `describe() -> String`. **Every create must be idempotent.** **There is no
  delete.**
- `ObservedRealm { display_name: String, roles: BTreeSet<RoleName>, clients: BTreeMap<OidcClientId, ObservedOidcClient> }`
  — only what the platform declares. Fields SaaS Fabric says nothing about are
  absent so the reconciler cannot notice, and therefore cannot overwrite, an
  operator's deliberate change to one.
- `ObservedOidcClient { redirect_uris: BTreeSet<RedirectUri>, public: bool, challenge_method: Option<PkceMethod>, audience_mapper: Option<String>, unmodellable_redirect_uris: usize }`.
  The last three are ADR 0019 §6: `challenge_method` is `None` for both "no
  such attribute" and "a value this model cannot read" — no `Plain` variant
  exists to hold the latter. `audience_mapper` is the audience the client's
  mapper currently asserts, or `None` if it has none. `unmodellable_redirect_uris`
  is a **count**, never the values — they are attacker-influenced text with no
  reason to reach a plan, a log line, or an API response — and non-zero is
  drift regardless of what `redirect_uris` holds.
- `ProviderError` — `Unavailable{detail}`, `NotPermitted`, `Rejected{detail}`.
  `is_transient()`. **`detail` must never carry an upstream response body or a
  credential** — the adapter is where that is dropped.
- `IdentityPlan` — `realm()`, `actions() -> &[IdentityAction]`, `is_converged()`.
- `IdentityAction` — `CreateRealm{display_name}`, `SetRealmDisplayName{display_name}`,
  `CreateRealmRole(RoleName)`, `CreateOidcClient(OidcClient)`, `UpdateOidcClient(OidcClient)`.
  Ordered: realm first, then roles, then clients.
- `IdentityReconciler::new(Arc<dyn IdentityProvider>)` —
  `plan(&Client) -> Result<IdentityPlan, ProviderError>` (observe only),
  `reconcile(&Client) -> ReconciliationOutcome` (observe, plan, apply).
- `ReconciliationOutcome` — `status()` (only `Applied` or `Failed`), `actions()`,
  `detail()`, `changed_nothing()`. Constructors `converged()`, `applied(n)`,
  `failed(&ProviderError)` are `pub` so other crates' tests can build one; there
  is deliberately no constructor for `Pending` or `Drifted`.
- `ReconciliationStatus` — `Pending | Applied | Failed | Drifted`. `as_str()`,
  `Display`, `Serialize` (lowercase).
- `ReconciliationReport { status, revision: ClientRevision, actions, observed_at_unix, detail }`.
- `ReconciliationStatusStore` — `report(&ClientId)`, `mark_pending(...)`,
  `record(...) -> ReconciliationStatus`. In-memory, `std::sync::Mutex` with
  poison recovery (no `unwrap`).
- `testing::FakeIdentityProvider` — `new()`, `fail_with`, `recover`, `calls`,
  `clear_calls`, `seed_realm`, `realm`.

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
5. **The diff compares role *containment*, not set equality.** Keycloak creates
   roles of its own; equality would try to "correct" every realm forever.
6. **Redirect URIs compare as sets, and set *equality*, not containment.**
   Order from a provider is arbitrary, so reordering is not a difference — but
   a legitimate extra URI the provider holds beyond what is declared is drift
   too, exactly as an unparseable one is (ADR 0019 §6).
7. **`Drifted` is decided by `status::transition`, from the previous report.**
   Not by the outcome, and not in two places.
8. **The audience is a provider fact, not a document field.** A client
   document has no way to name its own audience — see the design note below —
   so `matches` never reads one off `OidcClient`.

## Design notes

- The reconciler takes no clock. The caller stamps `observed_at_unix`, so the
  store and the audit record cannot disagree about when something happened.
- A declared client is always public, so an observed client held as confidential
  does not match and is corrected. That is the one property that changes what a
  client *is*.
- **`matches()`'s five terms** (`plan::diff::matches`): `existing.public`;
  `existing.unmodellable_redirect_uris == 0`; `existing.redirect_uris ==
  declared_uris(&declared.redirect)`; `existing.challenge_method ==
  Some(declared.pkce)`; `existing.audience_mapper.as_deref() ==
  Some(configured_audience)`.
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
  `configured_audience(&self) -> &str`, `IdentityReconciler::plan` reads it
  from the provider it already holds and passes it to `plan::plan`, and no
  change was needed to how a reconciler is constructed anywhere that builds
  one. That keeps a single source of truth for the string — the adapter that
  writes the mapper is the same object asked what it wrote — without adding a
  second, parallel configuration path into the control plane's composition
  root, and without this crate learning that a "Keycloak" exists: the trait
  method is phrased the same way `describe()` already is, as a fact a
  provider states about itself.
