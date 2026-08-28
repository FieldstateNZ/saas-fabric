# fabric-reconciliation — LLM context

Converges an identity provider onto a client's desired state. Depends on
`fabric-core`, `fabric-client-model`, `async-trait`, `serde`, `thiserror`,
`tracing`. **No HTTP, no provider protocol.** Event domain `11`.

## Public surface

- `IdentityProvider` (async trait, `Arc<dyn>`) — `observe_realm(&RealmName) -> Option<ObservedRealm>`,
  `create_realm`, `set_realm_display_name`, `create_realm_role`,
  `create_oidc_client`, `update_oidc_client`, `describe() -> String`.
  **Every create must be idempotent.** **There is no delete.**
- `ObservedRealm { display_name: String, roles: BTreeSet<RoleName>, clients: BTreeMap<OidcClientId, ObservedOidcClient> }`
  — only what the platform declares. Fields SaaS Fabric says nothing about are
  absent so the reconciler cannot notice, and therefore cannot overwrite, an
  operator's deliberate change to one.
- `ObservedOidcClient { redirect_uris: BTreeSet<RedirectUri>, public: bool }`.
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
6. **Redirect URIs compare as sets.** Order from a provider is arbitrary;
   treating a reordering as a difference makes every client permanently drifted.
7. **`Drifted` is decided by `status::transition`, from the previous report.**
   Not by the outcome, and not in two places.

## Design notes

- The reconciler takes no clock. The caller stamps `observed_at_unix`, so the
  store and the audit record cannot disagree about when something happened.
- A declared client is always public, so an observed client held as confidential
  does not match and is corrected. That is the one property that changes what a
  client *is*.
