# fabric-fga-auth

Verifies a tenant user's own token against a trusted issuer registry, binds
the verified identity to an authorization decision, and — via an embedded
OpenFGA — answers that decision. This is the library at the heart of the
Fabric authorization front door; `fabric-fga-auth-api` is the process that
hosts it.

Runtime-plane crate (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
Depends on nothing but `fabric-core`.

## Why this crate exists

A tenant user's token is issued by *their own client realm*. Every client has
a realm, so the runtime plane faces many issuers with many key sets — and
[ADR 0016](../../../docs/decisions/0016-fabric-owns-the-authorization-front-door.md)
found that neither OpenFGA nor Topaz can be pointed at that directly: OpenFGA
trusts exactly one issuer's key set, and Topaz's OIDC client refuses to fetch
discovery from a private address (which every in-cluster Keycloak is). So
Fabric owns the front door itself: this crate verifies the token against a
registry of trusted issuers (never discovery), binds the verified subject to
the exact identity a decision is made about, and only then reaches the
authorization service.

## What this crate does

Two responsibilities:

1. **Identity verification and binding** (`registry.rs`, `verifier.rs`,
   `cache.rs`, `keys.rs`, `http_keys.rs`, `identity.rs`, `windows.rs`) — the
   original and primary purpose: *given a token, who is this and which store
   answers for them?*
2. **Serving a decision** (`check.rs`, `openfga.rs`, `runtime.rs`) — the
   `RuntimeSurface` axum router that exposes `POST /v1/check`, binds the
   verified identity as the subject of the check, and asks an embedded
   OpenFGA for the answer.

Both are real, tested, and wired together by `fabric-fga-auth-api`.

## Key concepts

- **`Verifier::verify(token) -> Result<VerifiedIdentity, VerificationError>`**
  — reads the token's `iss` *unverified* (only to select a registration),
  looks up that issuer in the trusted `Registry`, checks the algorithm is one
  the registration permits, fetches (or reuses a cached) key by `kid`, and
  only then verifies signature/`iss`/`aud`/`exp`/`nbf`.
- **`Registry`** — built once at startup (`Registry::build`), never mutated.
  Refuses to build at all with zero issuers, a duplicate issuer, or a
  registration missing something it needs: an empty issuer, a tenant that is
  not a valid realm identity, an empty audience, an empty `jwks_uri`, no
  algorithms, an empty store, an empty `authorization_model_id`, or a
  `max_key_age_seconds` of zero. A registry that failed to build this way is
  exactly the shape in which at least one real authorization service quietly
  verifies nothing.
- **`VerifiedIdentity`** — `tenant`, `store` and `model` all come from the
  *registration* the verified issuer selected; only `subject` comes from the
  token's own claims. A token carrying a `tenant`, `store_id` or `principal`
  claim is not wrong to carry one — this crate simply never reads it. There
  is no public constructor outside the crate, so holding one is evidence a
  token was actually verified.
- **The key cache's two windows (`windows.rs`, `cache.rs`, `cache/held.rs`)**
  — an unknown `kid` is refused (`401`) only when a *fresh, successfully
  fetched* snapshot proves the issuer does not publish it
  (`UNKNOWN_KID_FRESHNESS_SECONDS`, 30s). A failed fetch never counts as
  evidence of absence and never renews that freshness window — it can only
  ever make the answer `Unavailable` (`503`), never `Refused` (`401`). A
  separate, longer bound (`max_key_age_seconds`, default 12 hours, per
  issuer) governs how long a cached key may still be used to *verify* a
  signature at all. A third window, `REFRESH_MIN_INTERVAL_SECONDS` (10s),
  bounds how often the issuer may be called at all — amplification
  protection, independent of whether calls are succeeding.
- **`Check` / `Decisions` / `OpenFgaDecisions`** — `CheckRequest { relation,
  object }` is Fabric's own request shape, with **no `user` field**: the
  subject of the check is always the caller's own `VerifiedIdentity`, never
  something a request body could name. `Decisions` is the port
  (`check`/`reachable`); `OpenFgaDecisions::on_loopback(port)` is the only
  implementation, and it is built from a **port number**, never a URL — there
  is no argument that could point it anywhere but `127.0.0.1`.
- **`RuntimeSurface`** — the axum router: `POST /v1/check`,
  `GET /health/live`, `GET /health/ready`. Deliberately tiny: one bearer
  scheme, no query-string alternative, a 2048-byte body limit, and a `404`
  for anything else. `allowed: false` is a `200`, never a `403` — the caller
  asked a question and got an answer.

## How the pieces fit

```text
Authorization: Bearer <the user's own JWT>
          |
          v
   fabric-fga-auth      unverified `iss` selects a Registry entry
          |              verify signature, iss, aud, exp, nbf, alg
          |              principal = SubjectId::from_verified(tenant, sub)
          v
   Check::run            binds principal as the check's subject
          |
          v
   OpenFgaDecisions      http://127.0.0.1:<port>/stores/{store}/check
          |
        OpenFGA           embedded, --authn-method=none, loopback only
```

## Getting started

`fabric-fga-auth-api` is the reference caller — see its `src/startup.rs`. In
short:

```rust,ignore
let registry = Registry::build(issuer_registrations)?;
let keys = Arc::new(KeyCache::new(Arc::new(HttpKeySource::new()?), clock));
let verifier = Arc::new(Verifier::new(registry, keys));

let decisions: Arc<dyn Decisions> = Arc::new(OpenFgaDecisions::on_loopback(openfga_port)?);
let check = Arc::new(Check::new(Arc::clone(&decisions)));

let router = RuntimeSurface::new(verifier, check, decisions).router();
```

## Common tasks

- **Adding a trusted issuer** — add an `IssuerRegistration` (tenant, issuer,
  audience, `jwks_uri`, algorithms, store, `authorization_model_id`,
  optionally `max_key_age_seconds`) to the deployment's configuration. There
  is no runtime API for this; the registry is built once at startup and
  fails closed if anything in it is unusable.
- **Understanding a `503` vs. a `401` from `/v1/check`** — see the module
  docs on `errors.rs` and `runtime/status.rs`: `401` means the presented
  credential was refused (bad signature, wrong audience, expired, unknown
  issuer, unknown key *proven* absent); `503` means trust could not be
  established (JWKS unreachable, cache too old) or the authorization service
  is unreachable. Never conflate the two — a `503` must never be reported as
  if the caller's credential were bad.
- **Adding a second decision operation** — read
  [ADR 0016](../../../docs/decisions/0016-fabric-owns-the-authorization-front-door.md)'s
  three-surface table (Decision / Relationship management / Administration)
  first. A *decision* operation binds the caller as the subject structurally
  (as `Check` does); anything that lets a caller name a different subject is
  a different kind of operation with its own authorization, never an
  optional field bolted onto this one.

## What this crate deliberately does not do

- **No policy of its own.** The authorization model lives entirely in
  OpenFGA, pinned per issuer by `IssuerRegistration::authorization_model_id`.
  This crate never chooses "latest" and never reads a model id from a token.
- **No destination the caller can influence.** `OpenFgaDecisions` is built
  from a loopback port, never a URL, config field, or request value.
- **No detail in a `401` response.** `RefusalReason` is logged and never
  returned — naming which check failed would tell an attacker what to try
  next.
- **Not wired into the shipped runtime plane yet.** Nothing in
  `fabric-data-api` or `fabric-api` depends on this crate — neither
  `Cargo.toml` names it. The `authorization-front` Docker stage (built from
  this crate via `fabric-fga-auth-api`) is also not among the images
  `.github/workflows/release.yml` publishes today (`runtime-api`,
  `control-plane-api`, `console` only). The library and its HTTP surface
  work end-to-end in their own tests; nothing in the shipped runtime plane
  requests a decision from it yet.

## Tests

- `src/**/*_tests.rs` — inline unit tests beside the module under test
  (`cache/cache_tests.rs`, `check/check_tests.rs`, `registry/registry_tests.rs`,
  `runtime/runtime_tests.rs`, `verifier/verifier_tests.rs`).
- `tests/check_over_the_wire.rs` — proves the OpenFGA adapter forwards
  trusted values (store, model, user, relation, object) to the wire
  unchanged, using deliberately mixed-case identifiers to catch an accidental
  `.to_lowercase()`, and that every service failure mode maps to
  `Unavailable`/`Internal` rather than to a denial. Runs against a real TCP
  socket (`tests/support/mod.rs`'s `FakeOpenFga`), not a mocked client.
- `tests/check_against_openfga.rs` — opt-in against a real OpenFGA
  (`FABRIC_TEST_OPENFGA_PORT`), skipped loudly when unset.
- `tests/whole_path.rs` — opt-in end-to-end (`FABRIC_E2E=1`) against real
  Keycloak + real OpenFGA + this crate's real HTTP surface; once enabled,
  missing setup is a failure, not a skip. Proves the issuer named in a token
  need not be the address keys are fetched from (the property that makes
  Topaz's private-address refusal irrelevant here).

## Gotchas

- The crate is `fabric-fga-auth` (hyphen); the Rust identifier — and the
  `RUST_LOG` filter target — is `fabric_fga_auth` (underscore):
  `RUST_LOG=info,fabric_fga_auth=debug`.
- A denial (`allowed: false`) is a `200`, never a `403`. `/v1/check` answers
  a question; `403` would claim the caller may not ask it at all, which
  sends an operator looking in the wrong place.
- `Registry::is_empty()` always returns `false` — a `Registry` cannot be
  built with zero issuers, so the method exists only because clippy expects
  it beside `len()`.
- `ObjectRef`'s reserved characters (`:`, `#`, `/`) are OpenFGA's and this
  platform's own syntax characters, not arbitrary restrictions — see
  `object.rs`. Its fields are private; the only way to read one back is its
  `Display`/`Serialize` (`resource:id`) or, for the resource half alone, the
  `resource()` getter.
- `unverified::issuer_of` is the one place in the crate that reads a claim
  before any verification — deliberately isolated in its own module so it
  stays the only place.
