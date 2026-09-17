# fabric-fga-auth — LLM context

Verifies a tenant user's token against a trusted issuer registry, binds the
verified identity to a `Check` decision, and calls an embedded OpenFGA for
the answer. Runtime-plane crate (see
`docs/architecture/crate-dependencies.md`). Depends only on `fabric-core`,
plus `async-trait`, `axum`, `base64`, `jsonwebtoken`, `reqwest`, `serde`,
`serde_json`, `thiserror`, `tokio`, `tower`.

Not wired into the shipped runtime plane: neither `fabric-data-api` nor
`fabric-api` depends on this crate. The `authorization-front` Docker stage
(built from this crate via `fabric-fga-auth-api`) is not among the images
`.github/workflows/release.yml` publishes (`runtime-api`, `control-plane-api`,
`console` only).

## Public surface (all re-exported from `lib.rs`)

- `VerifiedIdentity` — `tenant()`, `subject()`, `principal(): &SubjectId`,
  `store()`, `model()`. No public constructor; only `Verifier::verify`
  produces one (`pub(crate) fn new`).
- `Verifier::new(registry: Registry, keys: Arc<KeyCache>) -> Self` (`const`);
  `async fn verify(&self, token: &str) -> Result<VerifiedIdentity, VerificationError>`.
- `Registry::build(registrations: impl IntoIterator<Item = IssuerRegistration>) -> Result<Registry, ConfigurationError>`
  (the only constructor); `registration(issuer: &str) -> Option<&IssuerRegistration>`
  (exact match); `len() -> usize`; `is_empty() -> bool` (always `false`).
- `IssuerRegistration { tenant, issuer, audience, jwks_uri, algorithms:
  Vec<jsonwebtoken::Algorithm>, store, authorization_model_id,
  max_key_age_seconds }` (`serde::Deserialize`, `deny_unknown_fields`; all
  fields `pub`). `max_key_age_seconds` defaults to `43_200` (12h,
  `DEFAULT_MAX_KEY_AGE_SECONDS`, private) via `#[serde(default = "default_max_key_age")]`;
  every other field is required. `permits(Algorithm) -> bool`.
- `KeyCache::new(source: Arc<dyn KeySource>, clock: Arc<dyn Clock>) -> Self`;
  `async fn with_key<R>(&self, registration: &IssuerRegistration, key_id: &str,
  use_key: impl FnOnce(&DecodingKey) -> R) -> Result<R, VerificationError>` —
  the key is never handed out past the closure.
- `KeySource` (async trait, `Send + Sync`) — `async fn fetch(&self, jwks_uri: &str)
  -> Result<KeySet, String>`. `HttpKeySource::new() -> Result<Self, String>` is
  the only non-test implementation (10s timeout).
- `KeySet::from_jwks(document: &str) -> Result<KeySet, String>`; `get(&self, key_id: &str)
  -> Option<&DecodingKey>`; `contains(&self, key_id: &str) -> bool`; `len() -> usize`;
  `is_empty() -> bool`. RSA-only (`kty == "RSA"`); keys without a `kid`, `n` or `e`
  are dropped, not fatal. `from_entries` exists but is `#[cfg(test)] pub(crate)`,
  not part of the public surface.
- `ObjectRef` — private fields `resource: LogicalResourceName` (from
  `fabric-core`) and `id: String`; no `id()` getter, only `resource() -> &LogicalResourceName`.
  `parse(value: &str) -> Result<Self, String>` parses `"resource:id"`.
  `Display`/`Serialize` render the same `resource:id` form; `Deserialize` calls
  `parse`. Reserved chars in `id`: `:`, `#`, `/`, and any whitespace. Max id
  length 255 (`MAX_ID`).
- `CheckRequest { relation: RelationName, object: ObjectRef }` (`pub` fields,
  `serde::Deserialize`, `deny_unknown_fields`) — **no `user` field**.
- `Decisions` (async trait, `Send + Sync`) — `async fn check(&self, store: &str,
  model: &str, user: &str, relation: &str, object: &str) -> Result<bool, DecisionFailure>`;
  `async fn reachable(&self) -> bool`.
- `Check::new(decisions: Arc<dyn Decisions>) -> Self`; `async fn run(&self, identity:
  &VerifiedIdentity, request: &CheckRequest) -> Result<bool, DecisionError>` —
  builds `user = "user:{principal}"` (`USER_TYPE = "user"`) from the *verified*
  identity, never from the request.
- `OpenFgaDecisions::on_loopback(port: u16) -> Result<Self, String>` — base URL
  is always `http://127.0.0.1:{port}`; there is no other constructor. 5-second
  timeout (`TIMEOUT_SECONDS`). `check` does `POST {base}/stores/{store}/check`
  with `{"authorization_model_id": model, "tuple_key": {"user", "relation",
  "object"}}`; a transport failure or `5xx` is `Unavailable`, any other non-2xx
  or an unparseable body is `Internal`. `reachable` does `GET {base}/healthz`.
- `RuntimeSurface::new(verifier: Arc<Verifier>, check: Arc<Check>, decisions:
  Arc<dyn Decisions>) -> Self` (`const`); `.router(self) -> axum::Router` —
  `POST /v1/check` (`MAX_BODY_BYTES = 2048`, `413` on overflow rather than
  folded into `400`), `GET /health/live`, `GET /health/ready`.
- `ConfigurationError` (fatal at startup, `thiserror`) — `NoIssuers`,
  `DuplicateIssuer { issuer: String }`, `InvalidRegistration { issuer: String,
  detail: String }`.
- `VerificationError` — `Refused(RefusalReason)` (→ `401`) |
  `Unavailable(UnavailableReason)` (→ `503`).
- `RefusalReason` — `Malformed`, `NoIssuer`, `UnknownIssuer`,
  `DisallowedAlgorithm`, `BadSignature`, `UnknownKey`, `OutsideValidity`,
  `WrongAudience`, `NoSubject`, `UnusableSubject`. Logged, never returned to
  the caller.
- `UnavailableReason` — `KeysUnreachable`, `KeysTooOld`.
- `DecisionError` — `Unavailable` (→ `503`) | `Internal` (→ `500`; the
  platform's own state/request is wrong, never a denial).
- `DecisionFailure` — `Unavailable` | `Internal` (the adapter's vocabulary,
  translated 1:1 into `DecisionError` by `Check::run`).
- `REFRESH_MIN_INTERVAL_SECONDS: u64 = 10` (amplification-protection cooldown
  per issuer, in `windows.rs`); `UNKNOWN_KID_FRESHNESS_SECONDS: u64 = 30` (how
  long a successful snapshot proves a key's *absence*, also `windows.rs`).

## Internal modules

- `registry.rs` + `registry/registration.rs` — `Registry::build`'s startup
  checks (`fn check`); `IssuerRegistration`, `DEFAULT_MAX_KEY_AGE_SECONDS`
  (private, 43,200 = 12h).
- `verifier.rs` + `verifier/unverified.rs` — `Verifier::verify`;
  `CLOCK_SKEW_TOLERANCE_SECONDS = 30` (explicit override of `jsonwebtoken`'s
  60s default); `unverified::issuer_of` reads only `iss` from an unverified
  token, its own module so it stays the one place that does.
- `cache.rs` + `cache/held.rs` — `KeyCache`, `Entry` (`pub(super)`: per-issuer
  `snapshot: Option<Snapshot>` + `last_attempt_at: Option<u64>`, one
  `tokio::sync::Mutex<Entry>` per issuer inside an outer `Mutex<HashMap<...>>`),
  `Snapshot::is_stale` / `proves_absence`.
- `windows.rs` — the two public timing constants and why they must never
  merge.
- `keys.rs` — `KeySet`, `KeySource` trait, JWKS parsing (`JwksDocument`,
  `JwkEntry`, both private).
- `http_keys.rs` — `HttpKeySource`, the only production `KeySource`; 10s
  timeout (`TIMEOUT_SECONDS`).
- `identity.rs` — `VerifiedIdentity`.
- `object.rs` — `ObjectRef`; `RESERVED: [char; 3] = [':', '#', '/']`,
  `MAX_ID: usize = 255`.
- `check.rs` + `check/errors.rs` — `CheckRequest`, `Decisions`, `Check`,
  `DecisionError`, `DecisionFailure`; `USER_TYPE = "user"` (private).
- `openfga.rs` — `OpenFgaDecisions`, the only `Decisions` implementation;
  `TIMEOUT_SECONDS = 5` (private); `CheckResponse { allowed: bool }` (private).
- `runtime.rs` + `runtime/{bearer,health,status}.rs` — `RuntimeSurface`; the
  `POST /v1/check` handler (`async fn check`, private); `bearer::from` (strict
  `Bearer <token>` parsing, case-insensitive scheme, no query-string/cookie
  alternative); `health::{live, ready}` (`ready` checks `decisions.reachable()`,
  `live` checks nothing but the process); `status::{for_verification,
  for_decision}` (the HTTP-status mapping table, both `pub(super) const fn`).
- `errors.rs` — `ConfigurationError`, `VerificationError`, `RefusalReason`,
  `UnavailableReason`.

## Hard invariants — do not break

1. **The caller supplies a token and nothing else.** `tenant`, `store`,
   `model` and the realm half of `principal` come only from the
   `IssuerRegistration` the *verified* `iss` selected — never from a claim
   in the token, however plausible.
2. **A `CheckRequest` has no `user`/`store`/`tenant` field.** Binding the
   subject is structural (`Check::run` builds it from `VerifiedIdentity`),
   not a sanitisation step that could be forgotten.
3. **A failed key fetch can never become evidence of a key's absence.**
   `Entry::snapshot` is untouched on a fetch failure; only a *successful*
   fetch can prove a `kid` is unpublished, and only within
   `UNKNOWN_KID_FRESHNESS_SECONDS` of that success.
4. **`OpenFgaDecisions` is built from a port, never a URL.** There is no
   argument, config field, or per-request value that can point it off
   `127.0.0.1`.
5. **A denial is `200 {"allowed": false}`, never `403`.** `403` would claim
   the caller may not invoke the decision API at all.
6. **`RefusalReason` is logged, never returned.** The HTTP response for a
   `401` carries no detail about which check failed.
7. **`Registry` cannot be built empty, duplicated, or under-specified** — an
   empty-issuer authorization service is the exact failure mode ADR 0016
   documents another implementation making.
8. **Algorithms and the authorization model are pinned per issuer, never
   "latest" and never taken from the token header.** `Registry::build`
   refuses a registration with an empty `algorithms` list or empty
   `authorization_model_id`; `Verifier::verify` checks the token header's
   `alg` against `registration.permits(alg)` before any signature work.

## Notes

- `tests/check_over_the_wire.rs` proves the OpenFGA adapter forwards trusted
  values (store, model, user, relation, object) to the wire unchanged, using
  deliberately mixed-case identifiers to catch an accidental
  `.to_lowercase()`; runs against a real TCP socket fake
  (`tests/support/mod.rs`), not a mocked client.
- `tests/check_against_openfga.rs` — opt-in against a real OpenFGA
  (`FABRIC_TEST_OPENFGA_PORT`), skips (loudly) when unset.
- `tests/whole_path.rs` — opt-in end-to-end (`FABRIC_E2E=1`) against real
  Keycloak + real OpenFGA + this crate's real HTTP surface; once enabled,
  missing setup is a failure, not a skip. Proves the issuer named in a token
  need not be the address keys are fetched from (the property that makes
  Topaz's private-address refusal irrelevant here).
- `src/lib.rs`'s crate-level doc was corrected in this pass: it previously
  claimed the crate "talks to no authorization service at all", which
  `openfga.rs`/`check.rs`/`runtime.rs` (the embedded-OpenFGA decision surface)
  had already made false. It now describes both responsibilities.
