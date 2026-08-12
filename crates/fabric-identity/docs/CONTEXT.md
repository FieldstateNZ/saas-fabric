# fabric-identity — LLM context

Derives the tenant identity context from a bearer token. Event-ID domain **1**.
Depends on `fabric-core`, `axum`, `http`, `jsonwebtoken`, `base64`.

## Architectural stance

SaaS Fabric is **authentication-agnostic**. The edge authenticates; this crate
consumes the established identity and reads `tenant_id` from the token.
`TrustedIngressReader` is the canonical default. `ValidatingReader` is optional
defence in depth, never the recommended architecture and never the fix for a
missing network boundary.

## Public surface

- `TenantIdentity` — `tenant()`, `subject()`, `roles()`, `scopes()`,
  `has_role()`, `has_scope()`. Fields private; constructor `pub(crate)`.
  Implements `FromRequestParts<S>` where `Arc<IdentityResolver>: FromRef<S>`.
- `IdentityResolver::resolve(&HeaderMap) -> Result<TenantIdentity, IdentityError>`.
- `IdentityConfig` — `tenant_claim` (`tenant_id`), `subject_claim` (`sub`),
  `roles_claim` (`roles`), `reject_tenant_header` (`true`).
  `BANNED_TENANT_HEADER = "x-tenant-id"`.
- `IdentityError` — `MissingAuthorization`, `NotBearer`, `MalformedToken`,
  `UnverifiedToken`, `ExpiredToken`, `MissingTenantClaim{claim}`,
  `InvalidTenantClaim{claim}`, `TenantHeaderPresent{header}`. 401 for all except
  `TenantHeaderPresent` (400).
- `TokenReader` — `read()`, `describe()`. **Synchronous by design**: no I/O on
  the request path.
- `TrustedIngressReader::new(Arc<dyn Clock>)` — canonical. Parses claims,
  enforces `exp` with leeway. `describe()` is `"trusted-ingress"` — neutral, not
  a warning.
- `ValidatingReader::new(VerificationKeys)` — defence in depth. RS256/384/512,
  algorithms pinned (no `alg: none` downgrade). `.with_issuers()`,
  `.with_audiences()`, `.with_leeway_seconds()`.
- `VerificationKeys::from_jwks_json()` / `from_rsa_pem()`. A **snapshot** — no
  fetching, no discovery.
- `TokenClaims` — `string()`, `string_list()` (array or space-delimited),
  `unix_seconds()`, `raw()`.
- `encode_unsigned_token(&Map<String, Value>)` — test helper, exported because
  the Data API integration tests need it.
- `build_identity(config, reader) -> Result<Arc<IdentityResolver>, String>`.

## Module layout

```
readers.rs
  jwt_payload.rs      decode_payload   — segment split + base64 + JSON object
  expiry.rs           ensure_not_expired — applies in both postures
  unsigned_token.rs   encode_unsigned_token
  trusted_ingress.rs  the canonical reader
  validating.rs       the defence-in-depth reader
  validation_rules.rs baseline jsonwebtoken Validation (pinned algorithms)
  jwks.rs             JWKS document → RSA keys
  verification_keys.rs key selection by kid
bearer.rs             Authorization header → token
claims.rs  identity.rs  resolver.rs  extractor.rs  config.rs  errors.rs
logging.rs  registration.rs  token_reader.rs
```

## Hard invariants — do not break

1. **No code path reads a tenant from a header.** `reject_tenant_header` only
   controls whether the banned header's presence is a 400 or is ignored. (§11)
2. **Tenant comes from `config.tenant_claim` and nowhere else.** (§10)
3. **Every error is a rejection.** No default tenant, no fallback. (§28)
4. **No identity-provider-specific logic**, and no IdP *lifecycle* — no issuer
   discovery, no JWKS fetching, no realm knowledge. (§24)
5. **`TokenReader::read` stays synchronous.** Adding I/O here puts a network
   call on every request.
6. **Never log the rejected tenant value.** Log the claim name and the reason.
7. **Do not warn on correctly-configured trusted ingress.** It is the intended
   architecture, not a degraded mode.
