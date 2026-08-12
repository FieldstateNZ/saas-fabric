# fabric-identity

Answers one question: **which tenant does this request represent?**

## What this crate is not

It is not authentication. Authentication happens at the platform edge, in the
gateway, against whichever identity provider the deployment uses. By the time a
request reaches the runtime plane it has already been authenticated
(specification §8, §9).

This crate reads an *already-established* identity. Specification §12 is
explicit that parsing claims out of a token does not make a component
responsible for authentication.

## The flow

```
Authorization: Bearer <token>
        │
        ├─ 1. reject X-Tenant-Id if present        (§11)
        ├─ 2. extract the bearer token
        ├─ 3. read claims via a TokenReader
        └─ 4. tenant = claims["tenant_id"]         (§10)
        │
        ▼
   TenantIdentity { tenant, subject, roles, scopes }
```

Step 1 is first on purpose. A caller sending `X-Tenant-Id: globex` with a token
saying `acme` should be told it is wrong, not handed `acme` data and left
believing the header worked.

## Choosing a token reader — read this before deploying

`TokenReader` has two implementations and the choice is a real security
decision.

| Reader | Verifies signature | Use when |
|---|---|---|
| `ValidatingReader` | Yes | **Recommended.** Always, unless you have a specific reason not to. |
| `TrustedIngressReader` | No | The §9 posture: you are relying entirely on network controls to keep untrusted callers away from the runtime. |

### Why the default recommendation is to verify

§11 bans the `X-Tenant-Id` header so a caller cannot pick its own tenant. That
guarantee only holds if the token is trustworthy. If the runtime merely *decodes*
a token without checking its signature, then anything that can reach a runtime
pod — a server-side request forgery in a business application, a compromised
sidecar, lateral movement inside the mesh — can mint `{"tenant_id":"globex"}`
and be believed.

An unverified token is exactly as caller-controlled as the banned header. It
just looks official.

Verification costs a public-key operation against keys already in memory. No
network call, no identity-provider coupling — it needs a JWKS document, not a
vendor (§24).

`TrustedIngressReader` still rejects expired tokens, because that costs one
comparison.

## Getting started

```rust,ignore
let keys = VerificationKeys::from_jwks_json(&jwks_document)?;
let reader = Arc::new(
    ValidatingReader::new(keys).with_issuers(&["https://id.example.com".to_owned()]),
);

let resolver = build_identity(IdentityConfig::default(), reader)?;
```

Then make `Arc<IdentityResolver>` reachable from router state via `FromRef`, and
handlers can take a `TenantIdentity` parameter directly:

```rust,ignore
async fn list_customers(identity: TenantIdentity) -> impl IntoResponse {
    // identity.tenant() is the only tenant this request can mean.
}
```

Because it is an extractor, a handler cannot run without a resolved tenant.
"Did we remember to check the tenant?" becomes a compile-time question.

## Gotchas

- **The crate is `fabric-identity`; the log filter target is `fabric_identity`.**
  `RUST_LOG=info,fabric_identity=debug`.
- **`TenantIdentity` has no setter for `tenant`.** That is deliberate — §11
  requires one authoritative source. Authorization code may read `roles` and
  `scopes`, but per §23 nothing it decides can change the tenant.
- **A non-string `tenant_id` claim reads as absent**, not coerced. A token with
  `"tenant_id": 42` is a misconfigured provider and should fail loudly.
- **The rejected tenant value is never logged.** It is attacker-controlled;
  writing it into the log stream invites log injection and pollutes
  tenant-filtered queries.
- **`VerificationKeys` is a snapshot.** On key rotation, build a new one and
  swap the reader. Do not add a JWKS fetch to the request path — that is the
  same mistake as putting Git in the request path (§6).
- **With several keys configured and a token carrying no `kid`, verification
  fails.** Trying each key in turn would convert a misconfigured provider into a
  silent accept.
