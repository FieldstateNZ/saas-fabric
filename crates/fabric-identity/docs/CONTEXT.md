# fabric-identity — LLM context

Derives the tenant identity context from a bearer token. Event-ID domain **1**.
Depends on `fabric-core`, `axum`, `http`, `jsonwebtoken`, `base64`.

## Architectural stance

SaaS Fabric is **authentication-agnostic**. The edge authenticates; this crate
consumes the established identity. `TrustedIngressReader` is the canonical
default. `ValidatingReader` is optional defence in depth, never the recommended
architecture and never the fix for a missing network boundary.

**The tenant comes from the issuer's registration** (ADR 0019 §2), not from the
`tenant_id` claim. The resolver reads `iss`, looks it up in
`IdentityConfig.trusted_issuers` by exact match, and uses the registration's
tenant. The `tenant_id` claim stays **required** and must agree, as a consistency
check — it is never the source. No `iss`, an unregistered `iss`, or a
disagreeing claim are each a 401. An empty or duplicated registry is a startup
refusal, reached through `IdentityConfig::validate` from `build_identity`.

## Public surface

- `TenantIdentity` — `tenant()`, `subject()`, `roles()`, `scopes()`,
  `has_role()`, `has_scope()`. Fields private; constructor `pub(crate)`.
  Implements `FromRequestParts<S>` where `Arc<IdentityResolver>: FromRef<S>`.
- `IdentityResolver::resolve(&HeaderMap) -> Result<TenantIdentity, IdentityError>`.
- `IdentityConfig` — `tenant_claim` (`tenant_id`), `subject_claim` (`sub`),
  `roles_claim` (`roles`), `scope_claim` (`scope`), `reject_tenant_header`
  (`true`), `trusted_issuers` (**empty by default, and `validate` refuses
  empty**). `BANNED_TENANT_HEADER = "x-tenant-id"`.
- `TrustedIssuer` — `new(issuer, TenantId)`, `issuer()`, `tenant()`,
  `find(&[Self], issuer) -> Option<&Self>` (exact match). Deserialises from
  `{ issuer, tenant }`. Deliberately **not** `fabric-fga-auth`'s
  `IssuerRegistration`: no `jwks_uri`, no algorithms, no store — the canonical
  posture may not know those (§24).
- `IdentityError` — `MissingAuthorization`, `NotBearer`, `MalformedToken`,
  `UnverifiedToken`, `ExpiredToken`, `TokenNotYetValid`, `MissingIssuerClaim`,
  `UnregisteredIssuer`, `MissingTenantClaim{claim}`, `InvalidTenantClaim{claim}`,
  `TenantClaimDisagreesWithIssuer{claim}`, `TenantHeaderPresent{header}`. 401 for
  all except `TenantHeaderPresent` (400).
- `TokenReader` — `read()`, `describe()`. **Synchronous by design**: no I/O on
  the request path.
- `TrustedIngressReader::new(Arc<dyn Clock>)` — canonical. Parses claims,
  enforces `exp` **and** `nbf` with leeway. `.with_leeway(LeewaySeconds)`.
  `describe()` is `"trusted-ingress"` — neutral, not a warning.
- `ValidatingReader::new(VerificationKeys)` — defence in depth. RS256/384/512,
  algorithms pinned (no `alg: none` downgrade). `.with_issuers()`,
  `.with_audiences()`, `.with_leeway(LeewaySeconds)`.
- `LeewaySeconds` — checked clock-skew allowance shared by both readers.
  `try_new(u64) -> Result<Self, String>`, `DEFAULT` (60s), `MAX_SECONDS`
  (3600), `seconds()`. Both readers take this rather than a raw integer, so a
  value cannot narrow the window or grow large enough to neutralise it.
- `VerificationKeys::from_jwks_json()` / `from_rsa_pem()`. A **snapshot** — no
  fetching, no discovery.
- `TokenClaims` — `string()`, `string_list()` (array or space-delimited),
  `unix_seconds() -> Option<u64>`, `raw()`.
- `encode_unsigned_token(&Map<String, Value>)` — test helper, exported because
  the Data API integration tests need it.
- `build_identity(config, reader) -> Result<Arc<IdentityResolver>, String>`.

## Module layout

```
readers.rs
  jwt_payload.rs      decode_payload   — segment split + base64 + JSON object
  expiry.rs           ensure_not_expired — applies in both postures
  not_before.rs       ensure_already_valid — the mirror of expiry
  leeway.rs           LeewaySeconds — the checked skew allowance both share
  rejection.rs        classify() — jsonwebtoken error → IdentityError
  unsigned_token.rs   encode_unsigned_token
  trusted_ingress.rs  the canonical reader
  validating.rs       the defence-in-depth reader
  validation_rules.rs baseline jsonwebtoken Validation (pinned algorithms)
  jwks.rs             JWKS document → RSA keys
  verification_keys.rs key selection by kid
  posture_parity_tests.rs  holds the two postures against each other
claims.rs
  numeric_date.rs     to_numeric_date — JSON number → second, saturating
bearer.rs             Authorization header → token
identity.rs
  trusted_issuer.rs   TrustedIssuer — one issuer, one tenant, exact match
resolver.rs
  tenant_binding.rs   bind() — ADR 0019 §2 steps 3–6
extractor.rs  config.rs  errors.rs
logging.rs  registration.rs  token_reader.rs
```

## Hard invariants — do not break

1. **No code path reads a tenant from a header.** `reject_tenant_header` only
   controls whether the banned header's presence is a 400 or is ignored. (§11)
2. **Tenant comes from the issuer's registration and nowhere else.** The
   `tenant_id` claim is required and must agree; it is never the source, and a
   deployment with no registration for an issuer does not fall back to it — it
   refuses the token. (ADR 0019 §2, §7)
2a. **A token with no `iss` is refused**, not treated as unregistered. ADR 0002
   records the hole this closes.
2b. **The issuer match is exact**, never a prefix and never a pattern.
3. **Every error is a rejection.** No default tenant, no fallback. (§28)
4. **No identity-provider-specific logic**, and no IdP *lifecycle* — no issuer
   discovery, no JWKS fetching, no realm knowledge. (§24)
5. **`TokenReader::read` stays synchronous.** Adding I/O here puts a network
   call on every request.
6. **Never log the rejected tenant or issuer value.** Log the claim name and the
   reason. Both are attacker-controlled.
6a. **An empty `trusted_issuers` refuses to start**, and so do two registrations
   for one issuer. Neither is a state to discover from a request.
7. **Do not warn on correctly-configured trusted ingress.** It is the intended
   architecture, not a degraded mode.
8. **The two postures must agree on the validity window.** They check it by
   different means — this crate for trusted ingress, `jsonwebtoken` for defence
   in depth — so any change to how a `NumericDate` is read, rounded, or compared
   has to keep `posture_parity_tests` green. A fractional `exp`/`nbf` once made
   them disagree silently, and only the canonical posture was wrong.
9. **A present validity claim always constrains.** `unix_seconds()` yields a
   second for *every* JSON number, clamping ones outside `u64` rather than
   returning `None`. `None` means absent or non-numeric, and only that — the
   original bug was a spec-legal value reading as "no claim" and switching off
   its own check.
