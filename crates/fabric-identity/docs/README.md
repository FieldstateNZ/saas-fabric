# fabric-identity

Answers one question: **which tenant does this request represent?**

## What this crate is not

It is not authentication. Authentication happens at the platform edge, in the
gateway, against whichever identity provider the deployment uses. By the time a
request reaches the runtime plane it has already been authenticated (§8, §9).

This crate reads an *already-established* identity. §12 is explicit that parsing
claims out of a token does not make a component responsible for authentication.

## The architectural contract

```
internet
   → Envoy / gateway authenticates the caller and validates the bearer
   → ────────── platform trust boundary ──────────
   → SaaS Fabric consumes the established identity
   → tenant_id is read from the bearer token
```

SaaS Fabric is **authentication-agnostic**. The identity implementation can be
Keycloak, Entra ID, Auth0, a customer's own broker, or an OIDC broker in front
of several, and nothing in the runtime plane changes (§24).

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

## Token readers

| Reader | Verifies signature | Role |
|---|---|---|
| `TrustedIngressReader` | No | **The default and the canonical architecture.** |
| `ValidatingReader` | Yes | Optional defence in depth. |

### Trusted ingress is the normal posture

The gateway has already validated the token. Re-validating in the runtime
repeats work the edge exists to do, and pulls identity-provider concerns —
issuer discovery, JWKS lifecycle, realm knowledge — into a plane that §24
requires to stay independent of any identity implementation.

`TrustedIngressReader` parses claims and checks expiry. That is the whole job.

The posture depends on the other half of §9: protected runtime APIs must not be
reachable through an untrusted path, enforced with `NetworkPolicy`, private
cluster networking, service mesh policy, mTLS, or ingress-only exposure.

**If an untrusted client can reach the runtime directly, that is a network
policy failure, and it belongs to be fixed there.** Verifying signatures inside
the runtime would mask the failure while leaving every other unauthenticated
path into the plane wide open. Independent OIDC validation is not the
architectural answer to a missing boundary.

### When to add `ValidatingReader`

As a *second layer over* sound network policy — not instead of it. Reasonable
cases: a regulated environment where an auditor expects verification at more
than one hop, or a migration period where the ingress guarantee is not yet
fully trusted.

Even then the runtime takes on no identity-provider lifecycle. Keys come from a
JWKS document read once at startup; there is no discovery and no fetching.
Rotation means building a new `VerificationKeys` and swapping the reader.

## Getting started

The default posture needs no key material:

```rust,ignore
let reader = Arc::new(TrustedIngressReader::new(SystemClock::shared()));
let resolver = build_identity(IdentityConfig::default(), reader)?;
```

Defence in depth, where a deployment wants it:

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
- **`TenantIdentity` has no setter for `tenant`.** §11 requires one
  authoritative source. Authorization may read `roles` and `scopes`, but per §23
  nothing it decides can change the tenant.
- **A non-string `tenant_id` claim reads as absent**, not coerced. A token with
  `"tenant_id": 42` is a misconfigured provider and should fail loudly.
- **The rejected tenant value is never logged.** It is attacker-controlled;
  writing it into the log stream invites log injection and pollutes
  tenant-filtered queries.
- **Expiry is checked in both postures.** Replaying a captured expired token is
  cheap and refusing it costs one comparison.
- **With several keys configured and a token carrying no `kid`, verification
  fails.** Trying each key in turn would convert a misconfigured provider into a
  silent accept.
