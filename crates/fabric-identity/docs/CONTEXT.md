# fabric-identity — LLM context

Derives the tenant identity context from a bearer token. Event-ID domain **1**.
Depends on `fabric-core`, `axum`, `http`, `jsonwebtoken`, `base64`.

## Public surface

- `TenantIdentity` — `tenant() -> &TenantId`, `subject()`, `roles()`, `scopes()`,
  `has_role()`, `has_scope()`. Fields private; constructor is `pub(crate)`.
  Implements `FromRequestParts<S>` where `Arc<IdentityResolver>: FromRef<S>`.
- `IdentityResolver::resolve(&HeaderMap) -> Result<TenantIdentity, IdentityError>`.
- `IdentityConfig` — `tenant_claim` (default `tenant_id`), `subject_claim`
  (`sub`), `roles_claim` (`roles`), `reject_tenant_header` (`true`).
  `BANNED_TENANT_HEADER = "x-tenant-id"`. `validate()` rejects empty claim names.
- `IdentityError` — `MissingAuthorization`, `NotBearer`, `MalformedToken`,
  `UnverifiedToken`, `ExpiredToken`, `MissingTenantClaim{claim}`,
  `InvalidTenantClaim{claim}`, `TenantHeaderPresent{header}`. `IntoResponse`:
  401 for everything except `TenantHeaderPresent` (400).
- `TokenReader` trait — `read(&str) -> Result<TokenClaims, IdentityError>`,
  `describe() -> &'static str`. **Synchronous by design**: no I/O on the request
  path.
- `TrustedIngressReader::new(Arc<dyn Clock>)` — decodes, does *not* verify
  signatures; still enforces `exp` with leeway.
- `ValidatingReader::new(VerificationKeys)` — verifies RS256/384/512.
  `.with_issuers()`, `.with_audiences()`, `.with_leeway_seconds()`.
  Algorithms are pinned, not taken from the token header (`alg: none` downgrade
  defence).
- `VerificationKeys::from_jwks_json()` / `from_rsa_pem()`. RSA only; other key
  types skipped, not fatal.
- `TokenClaims` — `string()`, `string_list()` (array or space-delimited),
  `unix_seconds()`, `raw()`.
- `encode_unsigned_token(&Map<String, Value>) -> String` — test helper, exported
  because the Data API integration tests need it too.
- `build_identity(config, reader) -> Result<Arc<IdentityResolver>, String>`.

## Hard invariants — do not break

1. **No code path reads a tenant from a header.** `reject_tenant_header` only
   controls whether the banned header's presence is a 400 or is ignored; it
   never makes the header a tenant source. (§11)
2. **Tenant comes from `config.tenant_claim` and nowhere else.** (§10)
3. **Every error is a rejection.** No default tenant, no fallback. (§28)
4. **No identity-provider-specific logic.** No Keycloak realms, no vendor
   endpoints. Contract is: trusted bearer token + canonical tenant claim. (§24)
5. **`TokenReader::read` stays synchronous.** Adding I/O here puts a network
   call on every request.
6. **Never log the rejected tenant value.** Log the claim *name* and the reason.

## Notes for future work

- The `scope` claim name is hardcoded (unlike roles). Make it configurable if a
  deployment needs it.
- JWKS rotation currently requires rebuilding the reader. A background refresher
  swapping an `ArcSwap<VerificationKeys>` would mirror how
  `fabric-tenant-runtime` handles binding refresh — that is the right shape if
  it is needed.
