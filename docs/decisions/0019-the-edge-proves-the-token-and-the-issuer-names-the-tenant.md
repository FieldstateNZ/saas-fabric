# ADR 0019 — The edge proves the token, the issuer names the tenant, and a public client proves its code

- **Status:** Proposed
- **Date:** 2026-09-06
- **Applies to:** `fabric-identity`, `fabric-client-model`, `fabric-keycloak`,
  `fabric-reconciliation`, `fabric-control-plane`, `fabric-api`,
  `fabric-data-api`, `apps/control-plane-ui`, and the ingress the
  `saas-fabric-platform` repository deploys in front of every tenant runtime
  service
- **Related:** [Platform specification](../architecture/tenant-runtime-data-api.md)
  §8, §9, §10, §11, §12, §24, §28;
  [ADR 0002](0002-trusted-ingress-is-the-canonical-identity-model.md);
  [ADR 0008](0008-desired-state-is-the-authority.md);
  [ADR 0009](0009-operator-identity-is-not-tenant-identity.md);
  [ADR 0010](0010-operators-authenticate-against-the-platform-realm.md);
  [ADR 0015](0015-a-subject-is-named-by-its-realm.md);
  [ADR 0016](0016-fabric-owns-the-authorization-front-door.md);
  [ADR 0018](0018-runtime-state-is-published-as-three-versioned-documents.md);
  [The client desired-state document](../architecture/client-desired-state.md);
  [The test matrix for this decision](../architecture/identity-edge-test-matrix.md)

## Context

[ADR 0002](0002-trusted-ingress-is-the-canonical-identity-model.md) decided that
the tenant runtime consumes an identity the edge already established. In the
canonical posture `TrustedIngressReader::read` decodes the payload and runs
`window::ensure_current`, and does nothing else: no signature, no `iss`, no
`aud` (`crates/fabric-identity/src/readers/trusted_ingress.rs:92-99`, and
ADR 0002's posture table at
`docs/decisions/0002-trusted-ingress-is-the-canonical-identity-model.md:70-79`).

That decision is sound and this ADR does not revisit it. What it exposes is an
obligation ADR 0002 stated in prose and nothing has yet written down as a
contract:

> **On the tenant runtime Data API path, the edge is the only place a token's
> signature, issuer and audience are ever checked.**

### Two in-process verifiers exist, and neither is on this path

The repository is not free of token verification. It has two verifiers, each
authoritative for its own route, and naming them is what stops this ADR being
read as "SaaS Fabric never verifies anything".

- **`fabric-fga-auth`** verifies tenant users' tokens on the authorization
  front door's own route, `/v1/check`
  (`crates/fabric-fga-auth/src/runtime.rs:74-81`). It selects a registration
  from the unverified `iss`, refuses an algorithm the registration does not
  permit, then verifies signature, `iss`, `aud`, `exp` and `nbf` with a
  30-second skew tolerance (`crates/fabric-fga-auth/src/verifier.rs:25`,
  `:55-92`). ADR 0016 is its decision.
- **The operator OIDC verifier** in `fabric-control-plane` verifies operators'
  tokens on the control plane's own route. It pins its algorithm at
  construction, sets the issuer, deliberately switches `validate_aud` off
  (`crates/fabric-control-plane/src/operator/oidc/verification.rs:19-25`), and
  gates on `azp` instead
  (`crates/fabric-control-plane/src/operator/oidc.rs:113`). ADR 0010 is its
  decision.

Neither sits on `/v1/data/*` (`crates/fabric-data-api/src/routes.rs:21`). That
path runs the canonical posture, and its verification is the edge's job — which
is the half of the invariant this repository cannot supply.

### Today the obligation is declared and not proven

The runtime ships `mode = "trusted_ingress"` (`examples/config.toml:83`), and
the comment above it names the other half of §9 correctly. A search of this
repository for the resource that would enforce that half returns nothing: no
`SecurityPolicy`, no ext-auth filter, and the only `HTTPRoute` anywhere is a
patch target inside a Kustomize test fixture
(`crates/fabric-platform-git/tests/component_desired_state.rs:158`). A route
that forwards `/` with nothing validating in front of it is not trusted ingress;
it is an unauthenticated Data API. That is the critical blocker M2 exists to
close, and it is why §1 below is written as a contract the platform repository
implements rather than as code this one ships.

### The tenant is taken from a claim, and nothing binds that claim to anything

`IdentityResolver::resolve` is "the single place in the platform where a tenant
is decided" (`crates/fabric-identity/src/resolver.rs:12-24`). It reads the
configured claim, parses it as a `TenantId`, and that is the tenant
(`:50-64`). No issuer is consulted. So on this path a token that survives the
edge — genuine signature, genuine issuer — selects whatever tenant its
`tenant_id` claim names, and an identity provider that mints that claim from a
user-editable attribute is a cross-tenant read.

This is the reverse of what ADR 0016 already does one route over, where "only
`subject` comes from token claims" and the tenant comes from the registration
selected by the verified `iss`
(`docs/decisions/0016-fabric-owns-the-authorization-front-door.md:166-169`;
`crates/fabric-fga-auth/src/verifier.rs:98-101`). The Data API path should say
the same thing, and §2 makes it.

### The client model cannot say what M2 needs

M2 requires two real Keycloak realm users to authenticate through
authorization-code with S256 PKCE, from a native or mobile application. The
desired-state model cannot express that:

- every declared application client is reconciled as public —
  `declaration()` hard-codes `public_client: true`
  (`crates/fabric-keycloak/src/provider/mutate.rs:118-131`, at `bc1f58c`; the
  value at `:123`) — and **no field enforces PKCE at all**. The body Fabric
  writes carries `clientId`, `enabled`, `protocol`, `publicClient`,
  `standardFlowEnabled` and `redirectUris`, and nothing else
  (`crates/fabric-keycloak/src/wire/oidc_client.rs:34-63`, same commit). A
  public client with no PKCE requirement is a client whose authorization code
  is redeemable by anyone who intercepts it, which is the entire reason RFC
  8252 §8.1 requires PKCE for native applications;
- a redirect URI is a flat, validated string
  (`crates/fabric-client-model/src/identity/redirect_uri.rs`, at `bc1f58c`)
  that permits `https://` anywhere and `http://` on loopback or under
  `.internal` (`.../redirect_uri/authority.rs`, same commit). There is no way
  to say which of those a given client is *entitled* to, so a production client
  and a development client are indistinguishable in the document and either may
  carry the other's shape;
- a native application's private-use URI scheme
  (`nz.fieldstate.slipway:/callback`) is refused outright, because
  `authority::check` — the single function the rule lived in then — accepts only
  `https://` and `http://`. Slipway's desktop shell has no representable
  callback.

### And an out-of-band redirect URI can hide

`observe::clients` builds the observed URI set with
`.filter_map(|uri| RedirectUri::try_new(uri).ok())`
(`crates/fabric-keycloak/src/provider/observe.rs:75-79`, at `bc1f58c`). Any
URI the model cannot parse is silently dropped. `diff::matches` then compares
the surviving set against the declared set
(`crates/fabric-reconciliation/src/plan/diff.rs:76-80`), so an operator who adds
`http://evil.example.com/steal` to a realm by hand adds a URI the model refuses,
which is dropped on read, which leaves the sets equal, which reports
`matches` — and reconciliation never corrects it. The most dangerous redirect
URI is the one the safety check cannot see.

The roadmap requires this contract to merge **before** routes and mobile login
are implemented anywhere, so that neither is built against a shape that then
moves.

### Brett's decision, recorded on the issue on 2026-09-06

> Custom app schemes should be supported, but they are not a priority; if
> representing them in the model and the Keycloak reconciliation is difficult,
> defer the custom-scheme variant to a later phase. So: redirect strategy is a
> closed set that can express claimed-HTTPS / universal-link (the production
> rule), explicit development redirects, and a custom app scheme —
> representable from the start; whether reconciliation writes the scheme variant
> in this slice is the implementer's call, stated in the ADR with the reason;
> S256 PKCE required for every public client regardless; an unrepresentable or
> deferred redirect shape is refused with a message naming the phase, never
> coerced.

## Decision

### 1. The edge-trust contract for the tenant runtime Data API path

**Scope.** This section governs `/v1/data/*` — the tenant runtime Data API
served behind `fabric-identity`'s canonical reader. It does **not** govern
`fabric-fga-auth`'s `/v1/check`, which verifies for itself under ADR 0016, nor
the control plane's operator routes, which verify for themselves under
ADR 0010. Each of those is authoritative on its own route and neither is
weakened here. The runtime's liveness and readiness probes
(`crates/fabric-api/src/health/routes.rs:15-16`) are outside the protected path
and stay unauthenticated.

**Topology: one route per runtime service, serving many tenants.** A tenant
runtime service is not deployed per tenant, and the edge in front of it is not
a per-tenant object. There is **one** gateway route per runtime service, and it
carries **one** JWT policy whose `iss` allow-list is the **set** of issuers
registered for the tenants that service serves. The edge therefore answers
exactly one question about `iss` — *is this issuer one of the registered
issuers?* — and never *which tenant is this?* That second question is decided
at the runtime, by the binding in §2, from the same verified string. Splitting
the two is what keeps the edge a per-service artefact while the tenant boundary
stays a per-tenant fact.

**Every claim SaaS Fabric does not check on this path is checked before SaaS
Fabric, by the ingress, and the ingress is the only thing permitted to reach a
runtime service.** The platform repository implements this; this repository
states it, and its runtime configuration is only correct while it holds.

```text
untrusted network
   → gateway
        ├ verify signature against the issuing realm's JWKS
        ├ verify iss ∈ the registered issuer set (exact), aud, exp (required), nbf
        ├ project no claim into any header
        └ forward Authorization unchanged; strip everything else identity-shaped
   → ─────────── platform trust boundary ───────────
   → fabric-identity parses claims, re-checks exp and nbf,
     binds iss → tenant, and checks the tenant_id claim agrees
```

**What the ingress SHALL validate, and refuse the request if it fails:**

| Check | Rule |
|---|---|
| Signature | Verified against the **issuing realm's JWKS**, fetched and refreshed by the gateway. The logical issuer and the URL keys are read from are separate settings, for the reason ADR 0016's registry separates them (`crates/fabric-fga-auth/src/registry/registration.rs:17-27`): the browser is sent to a public issuer, the verifier reads keys from wherever it can actually reach. |
| Algorithm | An allow-list **per issuer**, never the token's own `alg` header, and never one global constant. The shape is `IssuerRegistration.algorithms` (`crates/fabric-fga-auth/src/registry/registration.rs:51-57`): each registered issuer names the algorithms acceptable for it. Refusing `alg: none` alone is not enough — a token perfectly signed with an algorithm nobody agreed to is still not acceptable (`crates/fabric-fga-auth/src/verifier.rs:66-71`). `fabric-identity`'s own `RS256/384/512` list (`crates/fabric-identity/src/readers/validation_rules.rs:8`) is **private to the defence-in-depth posture** and is not a value the platform may reference; it is named here only so nobody mistakes it for this contract. |
| `iss` | **Exact string match**, not prefix and not pattern, against the set of issuers registered for the tenants this route serves — "an issuer that matches loosely is an issuer somebody else can look like" (`crates/fabric-fga-auth/src/registry/registration.rs:39-43`). Required: a token omitting `iss` is refused, never waved through. Membership in the set is all the edge decides; §2 decides the tenant. |
| `aud` | Exact match against **the deployment's API audience string** for the tenant runtime Data API — one configured value, e.g. `saas-fabric-data-api`, not a per-client value and not a value any document sets. Required, on the same terms as `iss`. **Prerequisite:** the realm must actually mint it — see the mapper below. |
| `exp` | **Required**, and in the future. |
| `nbf` | If present, in the past. |
| Clock skew | See the three enforcement points below. The edge's leeway is **at most 30 seconds**. |

#### Two configurations of one fact, and which one is the authority

The gateway's issuer allow-list (in `saas-fabric-platform`) and the runtime's
`IdentityConfig.trusted_issuers` (§2) are **two configurations of one fact**.
They are generated from the same tenant list and they are not the same object,
so they can drift. What matters is the direction each drift fails in:

| Drift | Consequence |
|---|---|
| The gateway admits an issuer the runtime does not know | The token reaches the runtime and is **refused there** (§2). Fails closed. |
| The runtime knows an issuer the gateway does not admit | The token is **refused at the edge** and never arrives. Fails closed. |
| The runtime maps an issuer to the **wrong tenant** | This is the dangerous one, and no allow-list catches it — the gateway has no opinion about tenants. |

**The runtime's registry is the authority on which tenant an issuer names.**
The gateway's list can only ever ADMIT or REFUSE; it can never select. So the
third row is not an edge problem to be solved at the edge — it is a
misconfiguration of the tenant boundary itself, and the answer is to stop
writing the fact twice.

**Its eventual single source is the per-tenant runtime binding published under
[ADR 0018](0018-runtime-state-is-published-as-three-versioned-documents.md)** —
the tenants document, whose field table is at
`docs/decisions/0018-runtime-state-is-published-as-three-versioned-documents.md:404-414`.
An `issuer` field there would make the issuer→tenant binding travel with the
tenant it belongs to, published by the same reconciliation that publishes the
tenant's data bindings and revised by the same counter. That is a schema change
to a document whose unknown fields are "rejected, at every level"
(`docs/decisions/0018-…:416-419`), so it is a decision of its own and not a
line in this one.

**Until then** the binding is `[identity].trusted_issuers` in the runtime's
configuration, generated by the platform repository from the same tenant list it
generates the gateway's allow-list from. §G names the generation, not just the
two artefacts, because two hand-maintained lists are the shape the third row
above arrives in.

#### The audience: one string per deployment, and it must equal the front door's

ADR 0010 records the finding that makes a naive `aud` requirement fail:

> A realm mints access tokens whose audience is the resource server the caller
> asked for — commonly `account` — and names the client that obtained the token
> in `azp`. Requiring `aud` to equal the console's client id therefore refuses
> every genuine token until somebody adds an audience mapper, and the failure
> presents as a signature problem rather than a missing mapper.
> (`docs/decisions/0010-operators-authenticate-against-the-platform-realm.md:70-74`)

So requiring `aud` at the edge is only implementable if the realm is configured
to mint it. **The Keycloak client declaration therefore writes an audience
protocol mapper** — `oidc-audience-mapper`, with the included custom audience
set to the deployment's API audience string — for every declared public client;
it is read back on observation and it is part of `matches` (§6). The audience
value is configuration of the control plane's Keycloak adapter, not a field of a
client document: a document that could name its own audience would be a document
that could opt out of the edge's check.

**The equality constraint.** `fabric-fga-auth` already requires an audience of
its own, per issuer: `IssuerRegistration.audience`
(`crates/fabric-fga-auth/src/registry/registration.rs:45-46`) is put straight
into `validation.set_audience` and `aud` is a required claim there
(`crates/fabric-fga-auth/src/verifier.rs:79-80`). A client carries **exactly
one** audience mapper. Therefore:

> **The Data API's audience string and every `IssuerRegistration.audience` in
> the same deployment MUST be the same string.**

If they differ, the client's single mapper satisfies one route and not the
other, and **one of the two routes refuses every genuine token**. ADR 0010 has
already recorded what that looks like from the outside: it "presents as a
signature problem rather than a missing mapper". This is the second time the
same failure has been available to this platform, and stating the constraint is
cheaper than diagnosing it twice. A deployment that genuinely needs two audience
values needs two mappers, which is a change to the client model, not a
configuration choice.

#### Why `exp` is "required" at the edge and not merely "checked"

The runtime accepts a token with no `exp` at all. `expiry::ensure_not_expired`
returns `Ok` when the claim is absent
(`crates/fabric-identity/src/readers/expiry.rs:36-38`), and ADR 0002 records
that as deliberate: "The canonical posture accepts a token without one — the
edge already decided that token was good, and this reader's job is to consume
that decision, not to re-legislate the token's shape"
(`docs/decisions/0002-…:88-92`). That is the right division of labour and it is
not being reopened. Its consequence is that **the edge is the only enforcement
of `exp`'s presence on this path**: a bearer token with no `exp` never expires,
and if the gateway does not require the claim, nothing downstream will. The
matrix pins this with a test that asserts the runtime's acceptance, so nobody
later reads that acceptance as an oversight and nobody assumes the runtime is a
backstop.

#### Clock skew: three enforcement points, and one inequality

| Where | Value | Source |
|---|---|---|
| The edge | configured by the platform; **≤ 30 s** | this ADR |
| `fabric-identity` (Data API path) | `LeewaySeconds::DEFAULT` = 60 s, ceiling 3600 s | `crates/fabric-identity/src/readers/leeway.rs:65`, `:68` |
| `fabric-fga-auth` (`/v1/check`) | `CLOCK_SKEW_TOLERANCE_SECONDS` = 30 s | `crates/fabric-fga-auth/src/verifier.rs:25` |

**The rule: the edge's leeway is at most the smallest downstream leeway.**
Today that is 30 seconds. An edge more generous than a downstream hop admits
tokens that hop then refuses, and the symptom is intermittent `401`s at the
authorization front door that look like an outage rather than like a
misconfiguration. `fabric-identity`'s 60 seconds is therefore **never the
binding constraint** while the front door tolerates 30, and a deployment that
changes any of the three values must re-check the inequality — including the
case where a future deployment narrows `fabric-identity` below 30, which would
make it binding.

#### Three failure classes at the edge, and they are not the same status

ADR 0016 already settled this for the front door and the edge carries the same
table (`docs/decisions/0016-…:177-181`):

| Class | Causes | Result |
|---|---|---|
| configuration | no issuer registered for the route; a registration that is not usable | **refuse to start / refuse to serve the route** |
| credential | `iss` outside the registered set; malformed token; wrong `aud`; bad signature; unknown key proven absent by a fresh snapshot; expired or not-yet-valid; disallowed algorithm; missing `exp` | **`401`** |
| verification infrastructure | the realm's JWKS endpoint unreachable; refresh failed and no usable cached key; a refresh suppressed by the cooldown | **`503`, never `401`** |

**The `503` is not optional and it is not a nicety.** "A Keycloak outage must
not tell a legitimate user their credentials are invalid"
(`docs/decisions/0016-…:173-175`). The cache/cooldown split is ADR 0016's, and
the edge implements it as specified rather than as its gateway happens to
default: an unknown `kid` is a `401` **only** when a sufficiently fresh,
successfully fetched snapshot positively establishes the key is absent;
otherwise it is `503` (`docs/decisions/0016-…:185-202`, implemented in this
repository at `crates/fabric-fga-auth/src/cache.rs:92-142`). Two windows, not
one: the refresh cooldown is amplification protection and must never decide an
authentication result; the absence-freshness window is the security semantics
(`docs/decisions/0016-…:207-210`). A failed fetch updates the cooldown and never
the snapshot, so a failure can never age into grounds for a refusal.

#### What the ingress forwards, and what it strips

**What the ingress SHALL forward:** the `Authorization: Bearer <jwt>` header,
byte-for-byte as presented. Nothing is rewritten, re-minted, or re-signed. The
runtime reads the token and only the token.

**What the ingress SHALL NOT emit.** The gateway **MUST NOT project any
verified claim into a request header** — not through a `claim_to_headers`
mapping, not as `x-jwt-claim-*`, `x-auth-request-*`, `x-forwarded-user` or any
equivalent, and not under a name of the deployment's own choosing. This is a
prohibition on the *feature*, not a list of the spellings anybody thought of,
and it is stated separately from the strip below because the two answer
different questions: the strip defends against a **caller** that sends such a
header, and this defends against a **gateway** that is configured to add one.
A projected claim would be a second identity source arriving on the trusted
side of the boundary, which §11 forbids and which nothing downstream is written
to notice.

**What the ingress SHALL strip from every inbound request**, before forwarding:

- `X-Tenant-Id`, and any other tenant-selection header. Fabric refuses this one
  with a `400` (`crates/fabric-identity/src/config.rs:66`,
  `crates/fabric-identity/src/errors.rs:89`), and stripping it at the edge means
  a caller never gets as far as discovering it exists;
- **every** claim-projection header a caller could send — `x-jwt-claim-*`,
  `x-forwarded-user`, `x-auth-request-*` and equivalents. This is a prefix
  strip, not a list;
- the operator-plane identity header. Operator identity is a different
  mechanism reached through a different boundary (ADR 0009, ADR 0010) on a
  different service, and it must not be presentable on the product edge.

**What the ingress SHALL refuse: any path into a protected runtime API that
does not pass through it.** Two artefacts, both required, because they answer
different questions:

- a **route** that terminates at the gateway with the JWT policy above attached,
  so there is no route into `/v1/data/*` without it; and
- a **NetworkPolicy** (or the equivalent §9 control — private cluster
  networking, service mesh policy, workload identity, mTLS, ingress-only
  exposure; `docs/architecture/tenant-runtime-data-api.md:324-332`) restricting
  ingress to the runtime pods to the gateway's namespace, so a pod inside the
  cluster cannot skip the route.

**This is the half of the invariant the runtime cannot check for itself.** ADR
0002 says so plainly: "The runtime cannot detect its own exposure. A process has
no way to know whether it is reachable from an untrusted network"
(`docs/decisions/0002-…:139-141`). So it is a deployment gate or it is nothing.

#### The two `401`s differ, deliberately

| | Edge | Runtime |
|---|---|---|
| Body | none describing which check failed | a short message naming the cause — `bearer token has expired`, `bearer token has no tenant_id claim` (`crates/fabric-identity/src/errors.rs:13-77`) |
| `WWW-Authenticate` | present, `Bearer error="invalid_token"` | absent |

Each is right where it is. The edge faces the untrusted network, and naming the
failed check there tells an attacker which one to work on next — the same reason
`fabric-fga-auth` logs its `RefusalReason` and never returns it
(`crates/fabric-fga-auth/src/errors.rs:48-52`) and `rejection::classify`
collapses nearly everything into one opaque outcome
(`crates/fabric-identity/src/readers/rejection.rs:8-11`). The runtime faces a
caller that has already been authenticated by the edge, so its remaining
refusals are about the *shape* of an established identity — a missing tenant
claim, a claim that is not an identifier — and those messages "describe the
shape of the request, never the contents of the token"
(`crates/fabric-identity/src/errors.rs:99-101`). The challenge header belongs at
the edge because the edge is what a client re-authenticates against; emitting
one from the runtime would advertise a token endpoint the runtime does not own.

**A refusal at the edge is never a redirect to a login page.** The Data API is
an API, and a `302` to an identity provider is how a native client's token
refresh turns into an HTML page it cannot parse.

**What the ingress SHALL NOT do.** It does not authorize. Whether an
authenticated subject may perform an operation is Fabric's question and Fabric's
alone (ADR 0016), and an ext-auth filter that started answering it would be a
second policy decision point with no way to keep the two in step.

### 2. The tenant binding at the runtime

**The issuer determines the tenant. The `tenant_id` claim is a consistency
check, never the source.**

`IdentityConfig` gains `trusted_issuers`: a set of registrations, each binding
one exact issuer string to one tenant. `IdentityResolver::resolve` — the single
place a tenant is decided (`crates/fabric-identity/src/resolver.rs:12-24`) —
then does, in order:

1. reject a banned tenant header, as today (`:81-95`);
2. extract the bearer and read its claims through the configured `TokenReader`,
   as today (`:47-48`);
3. read `iss` from the parsed claims (`TokenClaims::string`,
   `crates/fabric-identity/src/claims.rs:32`);
4. look that issuer up in the registrations. **An issuer that is not registered
   is refused** — `401`, credential class;
5. take the tenant from the registration;
6. read the configured tenant claim. **It is required.** If it is absent, the
   token is refused, as it is today (`MissingTenantClaim`,
   `crates/fabric-identity/src/errors.rs:49-56`). If it is present and does
   **not** equal the registration's tenant, the token is refused — `401`,
   credential class.

Step 4 is the substantive change; step 6 is what makes it safe to keep the
claim at all.

**Why the claim stays required rather than becoming optional.** Once the issuer
names the tenant, a missing claim is technically resolvable — and resolving it
would be the convenient inference this platform refuses. A token minted without
the canonical claim is a token from a realm that has not been configured the way
§10 says a realm is configured, and admitting it would mean the platform
silently accepted two token shapes where the specification names one. Requiring
it also keeps the failure at the boundary: an identity provider that stops
emitting `tenant_id` is discovered on its first request, not on the day
somebody removes the registry entry that was quietly covering for it.

A token whose issuer says `acme` and whose claim says `globex` is not a request
to disambiguate; it is a request to pick, and picking is the bug.

This mirrors, on the Data API path, what ADR 0016 already guarantees on the
front door: "`tenant`, the realm identity and `store` come exclusively from the
registry entry selected by the verified `iss`. A token carrying plausible
`tenant`, `realm`, `store_id` or `principal` claims is not wrong to hold them —
they are simply never read" (`docs/decisions/0016-…:166-169`). The one
difference is that here the `iss` was verified by the edge rather than by this
process, which is exactly the dependency §1 exists to make explicit. The second
difference is that here the claim is *also* required to agree, because unlike
the front door this process cannot re-verify anything and a disagreement is the
only signal it will ever get that the edge and the registry have diverged.

**An empty registry refuses to start.** `IdentityConfig::validate`
(`crates/fabric-identity/src/config.rs:75-88`) gains this rule, and it is
reached from `build_identity`, which calls it before the resolver exists
(`crates/fabric-identity/src/registration.rs:19-28`, the call at `:23`). In the
runtime that is step 1 of the application graph — "Identity. First, because it
decides what the process will believe"
(`crates/fabric-api/src/startup/application.rs:42-47`). The reason is ADR
0016's: "An empty registry is the shape in which an authorization service
quietly trusts nothing — or, in at least one real implementation, quietly
trusts *everything*" (`crates/fabric-fga-auth/src/errors.rs:19-21`). Two
registrations naming the same issuer are likewise a startup refusal, because
which one wins would depend on map ordering
(`crates/fabric-fga-auth/src/errors.rs:25-32`).

**A token with no `iss` is refused**, not treated as unregistered-but-harmless.
This closes on the Data API path the same hole ADR 0002 records finding in the
defence-in-depth allowlists, where "any token that simply omitted `iss` sailed
past an issuer allowlist — a security control that silently did nothing"
(`docs/decisions/0002-…:96-101`;
`crates/fabric-identity/src/readers/allowlists.rs:5-21`).

**Both readers, and the deployment that configures issuers twice.** §2's
binding is a property of the *resolver* and is therefore independent of which
`TokenReader` a deployment runs — canonical or defence-in-depth. A deployment
running `ValidatingReader` consequently states its issuers in two places:
`[token].issuers`, the signature-verification allow-list whose presence makes
`iss` mandatory (`crates/fabric-api/src/config/token_config.rs:50-57`), and
`[identity].trusted_issuers`, the tenant binding. **They must name the same
set, and a divergence is a startup refusal.** The check belongs in
`AppConfig::validate`, which exists for exactly this — "relationships *between*
settings, and between settings owned by different crates"
(`crates/fabric-api/src/config/validation.rs:8-13`) — and which runs before the
application is built (`crates/fabric-api/src/main.rs:41-44`). Left unchecked,
an issuer in one list and not the other is a token that verifies and cannot be
placed, or a tenant binding for an issuer whose signature nobody will accept;
both are configuration errors that should not survive until a request.

**This is a binding, not a verification.** The runtime still does not check a
signature and still fetches nothing (§24, ADR 0002). It reads a string the edge
already proved and refuses to act on any other.

### 3. The native and public client contract

**Every application client SaaS Fabric declares is public and requires S256
PKCE.** There is no representable alternative, at either end.

- The document states the method: `pkce: s256`, and in `v2` the field is
  **required** (§5).
- `PkceMethod` is a single-variant enum whose only variant is `S256`, following
  `ClientProtocol`'s precedent
  (`crates/fabric-client-model/src/identity/oidc_client.rs:5-16`): a value the
  document says out loud, so a future method is an added variant that old
  documents keep parsing rather than a meaning they silently acquire.
- `plain` is not a variant. A document naming it fails to deserialise, so the
  refusal is a property of the type rather than a rule a validator has to
  remember. There is no code path, present or future, that can build an
  `OidcClient` requiring `plain`.
- `PkceMethod::as_wire_value()` lives in `fabric-client-model`, so the adapter's
  write and the reconciler's compare cannot disagree about the spelling.
- **The model declares only public clients.** `OidcClient` has no secret field
  and no `publicClient: false` path
  (`crates/fabric-client-model/src/identity/oidc_client.rs:20-27`), and
  `declaration()` hard-codes `public_client: true`
  (`crates/fabric-keycloak/src/provider/declaration.rs:70`). Confidential clients
  are **out of scope** here: they need a secret, secrets never enter desired
  state (§4), and ADR 0008 left secret delivery undesigned. The follow-on is
  named under "What this does not decide".

#### `RedirectUriKind`: scheme first, then host

Every redirect URI classifies into exactly one kind. The partition is two
levels deep, and both levels matter.

**Level 0 — normalise.** The scheme and the host are **lower-cased before every
test below**. RFC 3986 makes both case-insensitive, and at `bc1f58c` they were
compared case-sensitively at three points, all in `authority.rs` as it then
stood: `strip_prefix("https://")`, the `LOOPBACK` membership test, and the
`.internal` suffix test. So `http://LOCALHOST:5173/cb` was refused and
`https://ADMIN.CORP.INTERNAL/cb` was classified as an ordinary public host.
Neither is what the operator wrote and neither is what the identity provider
will do with it. This repository has already been bitten once by precisely this:
"a mixed-case fixture, absent, made a `to_lowercase()` bug invisible to both the
socket test and the real engine" (`docs/delivery.md:61-62`).

**Level 1 — the scheme decides which partition applies.**

| Order | Scheme | Kind |
|---|---|---|
| 1 | a private-use URI scheme in RFC 8252 §7.1 reverse-domain form (contains a dot; no `//` authority required) | `PrivateUseScheme`, **whatever its authority** |
| 2 | `http` or `https` | classified by host — level 2 |
| 3 | anything else | **refused**, not classified. `javascript:`, `data:` and `file:` are never private-use schemes and never reach level 2 |

**Level 2 — within `http`/`https`, the host decides.**

| Order | Kind | Test |
|---|---|---|
| 1 | `Loopback` | host is `127.0.0.1`, `::1` (bracketed as `[::1]` in a URI) or `localhost` — those three exactly |
| 2 | `PrivateNetwork` | host is `internal` or ends `.internal` — the ICANN-reserved TLD (`crates/fabric-client-model/src/identity/redirect_uri/host_kind.rs:104`) |
| 3 | `Https` | a **registered domain**, and only if the scheme is `https` — the positive rule below. Every IP address literal is refused, in every spelling, and so is every name under a top-level domain the public DNS has reserved. Plain HTTP that reaches here is refused |

**Scheme first is what makes `nz.fieldstate.slipway://localhost/cb` a
`PrivateUseScheme`** and not a loopback callback. A host-first partition would
classify it by the `localhost` in its authority and hand a native application's
private-use callback the entitlement a development HTTP callback has — two
different security arguments collapsed by a string that happens to appear in
both. The authority of a private-use URI is not a network location at all; it is
whatever the application put there, and RFC 8252 §7.1's own examples put nothing
there.

**Host second is the load-bearing part within `http`/`https`.**
`https://localhost:5173/cb` is `Loopback`, not `Https`;
`https://admin.corp.internal/cb` is `PrivateNetwork`, not `Https`. A
scheme-only partition would put both in `Https` and let a production strategy
hold a development callback while looking correct.

**The last arm is a positive rule, and that is the amendment.** It used to read
"everything else": a host was the production kind because no parser recognised
it as an address. A browser recognises more spellings than any parser does, so
the rule is now stated the other way round — a host is `Https` because it *is*
a registered domain
(`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:71`).
It is ASCII; it has at least two labels; each label is 1–63 characters of
letters, digits and hyphens and starts and ends with neither hyphen; the whole
name is at most 253 characters; and the final label is neither all-numeric nor
`0x`-prefixed — the URL Standard's "ends in a number" test, which is what makes
a host an IPv4 candidate to a browser rather than a name to resolve. **Why
positive:** a Universal Link and an App Link are claimed against a registered
domain, and an entitlement satisfied by an address that never leaves the
machine, or that only a resolver could recognise, is the entitlement failing to
mean anything.

**And a name nobody can register is not the production kind either.** `.local`
(RFC 6762) and `.test`, `.example` and `.invalid` (RFC 2606) are reserved
permanently: no registrar can sell one, and because none can ever be delegated
no public certificate authority will issue for one — so a callback on one can be
claimed as neither a Universal Link nor an App Link. All four are refused under
`https`, before the registered-domain rule runs, with a message each naming the
RFC that reserved it
(`crates/fabric-client-model/src/identity/redirect_uri/host_kind/special_use.rs:79`).
This is `.internal`'s own criterion read the other way round: `.internal` earns
a kind of its own **because** no publicly-trusted certificate can exist for it,
and these earn a refusal because a name nobody can hold is a name nobody can
prove they control. They classified as `Https` before this slice, which put a
client's production entitlement on a name that resolves to whatever the machine
in front of it decides.

Six spellings the negative rule admitted make the case. `https://0x/cb` and
`https://0x.0x.0x.0x/cb` — a browser reads an empty hexadecimal tail as 0, so
both dial `0.0.0.0`, the machine it is already on. `https://１２７．０．０．１/cb`
— UTS-46 maps the fullwidth digits back to `127.0.0.1`. `https://[foo]/cb` and
`https://[::1%25lo0]/cb` — a bracketed authority is an IPv6 literal or it is
nothing, and a zone id names an interface only one machine has.
`https://[::1/cb` — an unclosed bracket, which classified as **loopback** on
the strength of a bracket whose other half nobody wrote. The last three are
refused before the host rule, by `reject_brackets`
(`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:131`).

**A non-ASCII host is refused, and told to use its A-label.** `xn--` form is
what the browser resolves and what the claim is made against, so accepting a
U-label would mean this model and the operating system comparing two different
strings. The refusal names the encoding rather than the character.

**`ip_literal` keeps the job it is right for.** It is the loopback detector, and
it runs before the plain-HTTP arm for **both** schemes
(`crates/fabric-client-model/src/identity/redirect_uri/host_kind.rs:97`), so
`http://0x7f000001/cb` is refused as a loopback near-miss rather than as plain
HTTP on a public host. What it is not is the definition of "not a domain".

**Exactly three loopback hosts, and the near-misses are refused rather than
admitted.** `127.0.0.2` is loopback to the operating system and is **not** in
this list; `[::ffff:127.0.0.1]` is the IPv4-mapped spelling of one that is;
`localhost.localdomain` resolves to loopback on many machines. **So does every
name under `.localhost`** — RFC 6761 §6.3 requires it, and Chrome and Firefox
honour it without asking a resolver at all, so `https://app.localhost/cb` is the
machine the browser is already on wearing a name that looks registrable. All of
them are refused, under **both** schemes
(`crates/fabric-client-model/src/identity/redirect_uri/host_kind.rs:136`,
`.../host_kind/special_use.rs:64`), and the refusal names the boundary rather
than reading as a parse failure — the entitlement is a statement about a
*declared* callback, and a declaration that can only be recognised by resolving
a name is not a declaration. `[::ffff:127.0.0.1]` was **accepted** under
`https://` before this slice, because the `https` arm examined no host at all;
the narrowing is deliberate, because a claimed-HTTPS entitlement satisfied by an
address that never leaves the machine is the entitlement failing to mean
anything.

Three facts about today's parser that this changes:

- `::1` was not accepted. `LOOPBACK` held `["localhost", "127.0.0.1"]`, and an
  IPv6 literal was refused over plain HTTP by design. It is added, because RFC
  8252 §7.3 names it and a dual-stack development machine gets it from the OS;
  `LOOPBACK` is now those three spellings
  (`crates/fabric-client-model/src/identity/redirect_uri/host_kind.rs:31`). It
  required **bracket-aware host parsing**: `host` split on the first colon,
  which turns `[::1]:5173` into `[`, and now runs to the closing bracket
  (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:58`). The
  comment above it reasoned that the ambiguity did not matter *because* an IPv6
  literal is never loopback-by-name; that reasoning stopped being true here, so
  the comment carries the rule instead
  (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:42-52`).
- A `*` is permitted only in the final position, and **that is where it stayed**.
  An earlier draft of this decision widened it to admit `http://127.0.0.1:*/cb`
  as a second spelling of any-port. The real Keycloak says otherwise, so the
  widening was never landed: see the `Development` row below.
- The parser widens **universally** and the strategy narrows. A trailing
  wildcard is a spelling `RedirectUri::try_new` accepts anywhere; which
  strategies may *hold* one is §4's rule. Keeping the widening in one place and
  the entitlement in another is what stops the parser growing a second copy of
  the strategy table — but it means the parser's existing refusals are
  load-bearing, and the two that matter most are re-proved by mutation: a
  wildcard in the host (`https://*.example.com/callback`, refused at
  `crates/fabric-client-model/src/identity/redirect_uri/characters.rs:68` and
  again by the registered-domain rule) and a `javascript:` scheme (refused at
  `crates/fabric-client-model/src/identity/redirect_uri/kind.rs:109`). A
  private-use-scheme branch is the most plausible way to accidentally admit
  either.

#### The four variants, what each admits, and what each writes

| Variant | Admits | Wildcards | What Keycloak receives |
|---|---|---|---|
| `ClaimedHttps { uris }` | `Https` **only**. The production rule, and also what an iOS Universal Link and an Android App Link are. `https://localhost/cb` and `https://foo.internal/cb` are **refused under it**, because the partition classifies them elsewhere | **Refused.** RFC 9700 §2.1 requires exact redirect-URI matching, and Universal/App Links require exact URLs anyway | `redirectUris` verbatim; `pkce.code.challenge.method=S256`; `post.logout.redirect.uris=+`; the audience mapper |
| `PrivateNetwork { uris }` | `PrivateNetwork` over **http or https**. LucentRoot's production posture: "its gateway has one listener, on port 80, and its hosts are `*.lucentroot.internal`" (`docs/architecture/client-desired-state.md:140-141`) | **Refused**, same rule | as above |
| `Development { uris }` | `Loopback`. Over `http`, a URI registered without a port matches any port (Keycloak compares no port for it, RFC 8252 §7.3); over `https`, and whenever a port is written, the match is exact | A single **trailing path** `*` is permitted, here only. There is no wildcard **port**: `:*` is refused | as above; the portless spelling verbatim |
| `CustomScheme { scheme, uris }` | `PrivateUseScheme` matching its own declared `scheme` | Refused | **Nothing.** Refused at validation — see below |
| — (all variants) | — | — | post-logout is written once as `attributes["post.logout.redirect.uris"] = "+"`, Keycloak's documented value meaning "the registered redirect URIs". One list, so a second cannot drift out of step with it |

**`https://localhost/cb` is representable, and only under `Development`.** It is
not refused by the model — a developer running a local TLS proxy writes exactly
that — it is simply a `Loopback` URI, so the strategy that admits loopback is
the one that may hold it. Naming this explicitly because "the scheme is
`https`, therefore the strategy is `claimedHttps`" is the intuition the
partition exists to break.

**`PrivateNetwork` is a fourth variant, and it is a divergence from the three
the issue sketched.** This platform already has a deployment for which
plain-HTTP `.internal` is the *production* posture, not a development one.
Folding it into `Development` would make every LucentRoot client's document say
something false about itself; folding it into `ClaimedHttps` would put a
plain-HTTP URI inside the variant whose entire job is to be the HTTPS rule. A
closed set that cannot describe a deployment we already run is not closed
enough.

**Loopback, any port — as observed, not as assumed.** RFC 8252 §7.3 requires the
authorization server to allow any port for a loopback redirect, because a native
app binds an ephemeral one. An earlier draft of this row said `Development`
expresses that in **two** spellings: a URI with no port, and `*` in the port
position. The probe against a real Keycloak 26.0.8 on 2026-09-06 says otherwise,
and the row is amended with the evidence, which is the sentence above the table
this paragraph explains:

- Over `http`, a loopback URI registered **without a port**
  (`http://127.0.0.1/cb`, `http://localhost/cb`, `http://[::1]/cb`) matches the
  same path on any port. The path is still compared exactly — `/other` is
  refused.
- Over `https`, the port is always compared exactly: `https://localhost/cb` does
  not match `https://localhost:5173/cb`. So is a port written under `http`:
  `http://localhost:5173/cb` does not match `:9999`.
- **`http://127.0.0.1:*/cb` matches nothing at all** — neither `:54321/cb` nor
  the portless form. It is not a wider spelling of any-port; it is a redirect
  URI no browser will ever be sent to. So `:*` leaves the model: `RedirectUri`
  refuses it (`crates/fabric-client-model/src/identity/redirect_uri/characters.rs:56`)
  with a message that names what to write instead. The message is
  scheme-neutral, because `com.example.app://x:*` reaches the same refusal and
  its author is not writing an http loopback callback: it leads with the fact
  that holds for everyone — no identity provider matches a wildcard port — and
  then offers the portless loopback spelling, which is the one place a missing
  port means something.

Refusing it is the fail-closed reading. A spelling the identity provider matches
nothing against is worse than one it refuses here: the client would be written,
reconciled and reported converged, and the first login attempt would fail in a
different system, weeks later, with nothing in this repository saying why.

The findings are recorded in [`docs/verification.md`](../verification.md) beside
the 2026-08-28 run. Keycloak **26.0.8** is what was probed — the image
`scripts/e2e-services.sh` uses — and that is the whole of what has been
observed: one version, on one day, against one image. LucentRoot runs 26.7.2,
and nothing here has been run against it. §G17 is the obligation to do that, and
it is open.

**A URI whose kind is not admitted by its declared strategy is refused, not
reclassified.** `http://localhost:5173/callback` under `claimedHttps` is an
error naming the strategy, the URI's kind, and what the strategy admits. This is
the whole value of the enum: today the two are indistinguishable and a
production client may quietly hold a development callback.

#### `CustomScheme` is representable now and refused at validation

Per Brett's decision above, the shape is in the model from the start;
reconciliation does not write it in this slice. The refusal happens at
**validation** — the same three-point schedule every other rule runs on
(`crates/fabric-client-model/src/identity/validation.rs:13-17`) — so the
document is never written, rather than being written and then failing at the
adapter. Three reasons, in order of weight:

1. **Representing it is not the hard part; getting it right is.** Keycloak would
   accept the string. What needs design is the matching semantics that make the
   scheme *safe*: `com.example.app:/callback` and `com.example.app://callback`
   are matched differently by Keycloak, by AppAuth-Android and by
   `ASWebAuthenticationSession`, and a scheme any other application on the
   device can also register is precisely the interception RFC 8252 §8.6 warns
   about. That is a decision with its own evidence, not a line in a serialiser.
2. **It has no consumer in this slice.** M2's acceptance is two real Keycloak
   users signing in through authorization-code with S256 PKCE. Synthesis Cloud's
   mobile client reaches its BFF over claimed HTTPS; Slipway is a desktop shell,
   and RFC 8252 §7.3 recommends a loopback redirect *over* a private-use scheme
   for exactly that case.
3. **Writing a variant nobody has signed in through would assert a security
   boundary with no evidence behind it**, which is the failure
   [`docs/delivery.md`](../delivery.md) exists to prevent.

Fixing the *shape* now is what makes the deferral cheap: phase 2 deletes one
validation rule and adds one adapter arm.

**Serde: values lowercase, keys camelCase.** `pkce: s256` and
`strategy: claimedHttps` follow the two conventions the document already uses —
values are the lowercase spelling `ClientProtocol` established (`type: oidc`),
and a multi-word value takes the camelCase the keys use, because `claimedhttps`
is not a word and a hyphen would be a third convention.

```yaml
clients:
  - id: web
    type: oidc
    pkce: s256
    redirect:
      strategy: claimedHttps
      uris:
        - https://www.example.com/callback
```

### 4. Validation, fail closed, and the statuses

Every rule below is refused before the document is written, and again when a
stored document is read, on the existing three-point schedule
(`crates/fabric-client-model/src/identity/validation.rs:13-17`).

1. A `pkce` value other than `s256` — refused by deserialisation.
2. `pkce` absent from a `v2` document — refused as a missing field (§5).
3. A redirect URI whose `RedirectUriKind` is not admitted by its declared
   strategy — refused, naming the strategy, the kind, and what the strategy
   admits.
4. A wildcard under `ClaimedHttps` or `PrivateNetwork` — refused, naming
   RFC 9700 §2.1's exact-matching requirement.
5. **A wildcard in the port position, under every strategy** — refused by the
   parser rather than by a strategy rule
   (`crates/fabric-client-model/src/identity/redirect_uri/characters.rs:56`).
   An earlier draft of this list made `:*` a `Development`-only spelling meaning
   "any port on loopback". The probe against a real Keycloak 26.0.8 found it
   means nothing anywhere — `http://127.0.0.1:*/cb` matches no redirect at
   all — so there is no strategy left for it to be entitled under, and the rule
   belongs where a spelling nobody matches belongs: at the parser, refused for
   every scheme and every strategy alike. §3's `Development` row is the same
   fact from the other side: any-port is the **portless** spelling, and a
   written port is compared exactly.
6. A `redirect` with an empty `uris` list — refused, as an empty `redirectUris`
   was (`crates/fabric-client-model/src/identity/validation.rs:78-90`, at
   `bc1f58c`): a client with no callback can never sign anyone in.
7. `strategy: customScheme` — refused with a message naming
   **`Lane E phase 2`** and a representable alternative.
8. A scheme or host the model cannot classify at all — `javascript:`, `data:`,
   `file:`, a wildcard in the host, userinfo in the authority, one of §3's
   loopback near-misses — refused, each by the rule that owns it: the scheme by
   `kind::classify`
   (`crates/fabric-client-model/src/identity/redirect_uri/kind.rs:109`), the
   wildcard by `characters::check`
   (`crates/fabric-client-model/src/identity/redirect_uri/characters.rs:68`),
   userinfo by `authority::reject_userinfo`
   (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:100`),
   and the near-misses by `host_kind::classify`
   (`crates/fabric-client-model/src/identity/redirect_uri/host_kind.rs:97`). No
   new shape is coerced into an existing variant.
9. A `v2` document still carrying the pre-migration `redirectUris` key — refused
   before the typed deserialisation, with a message naming `redirect` and
   `strategy`. Checked in `document/parse.rs` beside `check_document_kind`, for
   the reason that already lives there: "the first message points an operator at
   the actual problem, the second sends them looking for a field the document
   was never supposed to have"
   (`crates/fabric-client-model/src/document/parse.rs:16-20`).

Nothing in this list is defaulted, coerced, or repaired. A shape the model
cannot represent is an error the operator sees.

**The two statuses, and why a validation refusal has both.** Every rule above is
a `DesiredStateError`, and the status depends on which side of the boundary the
document came from:

- **On the write path** — an operator submitting a change through the API — it
  is `ControlPlaneError::InvalidRequest`, **`400`**, machine code
  `invalid_request` (`crates/fabric-control-plane/src/errors/status_mapping.rs:50-53`,
  `.../status_mapping/codes.rs:15`).
- **On the read path** — a stored document in `saas-fabric-clients` that this
  model refuses — it is `InvalidDesiredState`, **`500`**, machine code
  `desired_state_invalid` (`.../status_mapping.rs:80-82`, `codes.rs:16`), because
  "a stored document that will not parse is the platform's problem, not the
  caller's, and no retry fixes it".

This matters for `customScheme` specifically: an operator cannot write one
(`400`), and a document that somehow acquired one out of band makes the whole
client unreadable (`500`) — not a client with a warning. That asymmetry is the
reason §5 keeps `v1` parsing rather than breaking it.

### 5. Schema: `v2` beside `v1`

**`apiVersion: fabric.fieldstate.nz/v2` ships alongside `v1`.** This is the rule
the repository already wrote down, being exercised rather than amended:

> Versioned from the start, and checked on read. A future change to the
> document's shape ships as `v2` alongside this one rather than reinterpreting
> documents already in the repository — the same policy the Data API applies
> to its path prefix, for the same reason: the repository holds documents
> nobody is going to migrate on a schedule the platform controls.
> (`crates/fabric-client-model/src/document/schema.rs:6-11`)

and, in the operator-facing contract:

> A change to this format ships as `v2` alongside `v1`, never as a
> reinterpretation of documents already in the repository.
> (`docs/architecture/client-desired-state.md:70-71`)

Neither sentence is edited by this ADR. `check_document_kind` gains a second
accepted pair rather than a replaced one
(`crates/fabric-client-model/src/document/parse.rs:46-67`).

**What `v2` carries.** `pkce` **required**, no default; `redirect` carrying the
strategy; `redirectUris` gone. Requiring `pkce` follows `ClientProtocol`'s
precedent rather than contradicting it — a defaulted field is a meaning a
document acquires without saying it, and the whole point of the field is that
the document says it.

**How a `v1` client is read: one narrow migrator, stated here in full.** Every
`redirectUris` entry is classified by `RedirectUriKind`, and then:

| All entries classify as | Becomes |
|---|---|
| `Https` | `ClaimedHttps { uris }` |
| `PrivateNetwork` | `PrivateNetwork { uris }` |
| `Loopback` | `Development { uris }` |
| a **mix** of the above | **refused**, naming `v2` and `spec.identity.clients[].redirect` |
| any `PrivateUseScheme` | **refused**, naming `v2` and the field |

The mixed case is refused rather than resolved because there is no honest
resolution: a client holding both a production callback and a loopback one is
the exact ambiguity the strategy exists to remove, and a migrator that picked
the looser one would silently grant the entitlement the operator never stated.
The private-use case cannot arise from a document `v1` could hold —
`authority::check` refuses those schemes today — but the arm is written anyway,
so the migrator stays total once `RedirectUri` widens in this same slice.

#### A write to a `v1` document migrates it to `v2`, in place

An operator who edits a `v1` client's identity through the control plane gets a
`v2` document back. `with_identity` rewrites `apiVersion` to
`fabric.fieldstate.nz/v2` and writes the `v2` client shape.

This is **mechanically forced, not a preference.** `with_identity` merges the
new `spec.identity` into the raw document and then re-parses the whole rendered
text — "it means a `ClientDocument` handed to a repository has been read by
exactly the code that will read it back, so there is no path that produces a
document this model would later refuse"
(`crates/fabric-client-model/src/document/render.rs:22-27`, the render-then-parse
at `:50-51`). A `v2` `identity` block under a `v1` `apiVersion` would fail that
re-parse. So the edit either migrates the document or it fails; there is no
third behaviour to choose between, and the one that fails would make `v1`
documents read-only for no reason anybody asked for.

Two things follow, and both are the point:

- **It is an explicit migration performed by an edit, never a reinterpretation.**
  The operator asked for a change and receives a document that says `v2` at the
  top. A `v1` file nobody has edited stays `v1`, stays labelled `v1`, and is
  still read through the migrator above. Nothing reinterprets a document at
  rest, which is the guarantee `schema.rs:6-11` states.
- **It is confined to documents an operator actually changed.** The control
  plane "rewrites **only documents an operator has actually changed** rather
  than normalising the repository on read"
  (`docs/architecture/client-desired-state.md:183-186`), so no sweep migrates
  anything.

`apiVersion` keeps its position in the file: an ordered mapping's `insert`
replaces in place (`crates/fabric-client-model/src/document/render.rs:45-48`),
which is also what the existing key-order guarantee depends on.

**The console shows the document's version, and says that an edit will migrate
it.** An operator should not discover the version change in a Git diff.

**`v1` clients are reconciled with S256.** This is the deliberate runtime break:
a `v1` document that is never edited is never rewritten, but the next sweep
writes `pkce.code.challenge.method=S256` and the audience mapper to its clients
in Keycloak. Every public client that was not already performing PKCE stops
working until it does. That is the point of the slice; it is stated here, and it
must be said to client teams before the sweep rather than after.

**`v1` is deprecated** and the console shows the document's version, so an
operator can see which clients are still on it.

**The `redirectUris` migration pre-check (§4 rule 9) applies to `v2` documents
only.** In a `v1` document `redirectUris` is not a mistake, it is the schema.

#### The shipped examples

Three documents, chosen so that every parse path is exercised by the corpus a
test already walks (`crates/fabric-control-plane-api/tests/example_configuration.rs:92-110`
reads the whole `examples/clients` directory):

| Document | Version | Why |
|---|---|---|
| `examples/clients/acme.yaml` | **migrated to `v2`** | The repository must ship an example of the shape it now recommends. `examples/clients/acme.yaml:31-35` becomes a `pkce` and `redirect` block |
| `examples/clients/initech.yaml` | **new, stays on `v1`**, one client with public callbacks | The migrator needs shipped-corpus coverage. Without it the only `v1` example declares `clients: []` and the migrator is never reached by an example at all |
| `examples/clients/northwind.yaml` | **untouched, `v1`** | It declares `clients: []` (`examples/clients/northwind.yaml:26`) and therefore needs no change. The cheapest possible proof that a `v1` document still parses |

### 6. Drift, and what observation reports

**An observed redirect URI the model cannot parse is drift, not silence.**
`observe::clients` dropped it
(`crates/fabric-keycloak/src/provider/observe.rs:75-79`, at `bc1f58c`), which
made the most dangerous kind of out-of-band edit the one kind reconciliation
could not see. Observation therefore reports, beside the parsed set, **the
count of observed URIs the model could not parse**, and `matches` is false
whenever that count is non-zero
(`crates/fabric-reconciliation/src/plan/diff.rs:76-80` gains the term).
The resulting `UpdateOidcClient` rewrites the declared set, which removes the
unmodellable entry.

**The count lives on `ObservedOidcClient`** (`crates/fabric-reconciliation/src/provider/observed.rs:36-47`),
not on `ObservedRealm`. It is a per-client fact and the decision it feeds is a
per-client decision: a realm-level total would say a realm has drifted without
saying which client to rewrite, and the reconciler would have to rewrite all of
them or guess.

A count, not the values: the values are attacker-influenced strings and the
platform has no reason to carry them into a plan, a log line, or an API
response. `logging::unmodellable_role_ignored` is the existing precedent for the
role side — and the difference is exactly the one that file already records. A
declared role "always parses — it came from a document this platform validated.
So a name that fails to parse is by definition one SaaS Fabric did not declare,
and one it will therefore never look for. Dropping it changes no decision the
reconciler makes" (`crates/fabric-keycloak/src/provider/observe.rs:89-95`, at
`bc1f58c`). An
unmodellable *redirect URI* is precisely a decision the reconciler must make,
so the same reasoning gives the opposite answer.

**`ObservedOidcClient.challenge_method: Option<PkceMethod>`.** Typed, not a
`String`: a value Keycloak holds that this model cannot parse — `plain`, empty,
a typo — reads as `None`, which is not `Some(S256)`, which is drift. So the
downgrade case needs no `Plain` variant anywhere in the model to be corrected.

**The audience mapper is read back and is part of `matches`.** A client whose
mapper was removed by hand stops matching and is rewritten. Without this the
mapper would be written once and could silently disappear, taking the edge's
`aud` check down with it — the write/read asymmetry that hides forever.

**What Keycloak actually does here is verified, not asserted.** At `bc1f58c`,
`ClientRepresentation` read four fields and neither `attributes` nor
`protocolMappers` was among them
(`crates/fabric-keycloak/src/wire/oidc_client.rs:5-26`, same commit), so
nothing in this repository had ever observed either. The socket-level fake
will return whatever the test hands it, which means a fake alone can prove
the adapter parses a response and can prove nothing about whether Keycloak
sends one. Three questions
are therefore settled against a **real Keycloak** in the adapter slice, and
recorded in [`docs/verification.md`](../verification.md) beside the 2026-08-28
findings:

1. does `GET /admin/realms/{realm}/clients` return `protocolMappers` and
   `attributes` at all, or does it require a per-client read;
2. does `PUT /clients/{id}` carrying `protocolMappers` **update** them, or is
   the `/clients/{id}/protocol-mappers/models` sub-resource required — if so,
   `admin/paths.rs` gains that path and the update grows a second call;
3. what Keycloak accepts as an any-port loopback redirect (§3).

If (1) or (2) contradicts this section, this section is amended in that slice
with the evidence. The pattern is the one that section of `verification.md`
exists for: "Two things only the real instance could have told us"
(`docs/verification.md:519-534`).

**`/clients` is read with a bounded page, exactly as roles are.** At
`bc1f58c`, `paths::clients` had no bound
(`crates/fabric-keycloak/src/admin/paths.rs:61-64`) while `paths::roles_page`
did (`:53-59`), and `observe::roles` refused a response that reached the cap
rather than reconciling against a truncated list
(`crates/fabric-keycloak/src/provider/observe.rs:12-18`, `:46-50`, same
commit). Clients get the same treatment and the same refusal. Quietly working
from a partial client list would leave a realm permanently reporting changes
it had already made, and — now that an unparseable URI is drift — could also
hide the client carrying
one.

### 7. The identity-source rule, rewritten

- **The tenant comes from the issuer's registration** (§2). Not from a header,
  query parameter, body field, path segment, or client identity — and, as of
  this decision, not from a claim either.
- **The `tenant_id` claim is required and must agree with the registration, or
  the token is refused.** It is a consistency check. It is never the source, and
  a deployment that has no registration for an issuer does not fall back to it —
  it refuses the token.
- **`X-Tenant-Id` stays refused with a `400`, and is also stripped at the edge.**
  Both, not either: stripping is the defence, refusing is the diagnostic that
  tells a caller their assumption is wrong. The refusal is a configuration
  default — `reject_tenant_header` defaults to `true`
  (`crates/fabric-identity/src/config.rs:39-49`, `:59`). This ADR does **not**
  require that default to hold in every deployment, because §G9 strips the
  header before it can arrive: with the strip in place the switch decides only
  what a request that never reaches the runtime would have been told, and
  demanding a setting whose effect the boundary has already removed is a rule
  nothing would enforce and nothing would notice breaking. The header is never
  *read* as a tenant source regardless of the setting, and no code path does so
  (`crates/fabric-identity/src/resolver.rs:78-80`); that is the invariant, and it
  is not configurable. `true` remains the shipped default and the recommended
  posture, because a caller who believes the header works should be told.
- **`azp` and `client_id` are not identity inputs on the Data API path.**
  Nothing added here lets a caller name its own client identity, and a redirect
  strategy is desired state an operator wrote rather than something a request
  carries. **The named exception is ADR 0010's operator plane**, where `azp` is
  deliberately the gate — "was this token obtained by the console, or by another
  client in the same realm whose holder is now presenting it here?"
  (`docs/decisions/0010-…:76-78`;
  `crates/fabric-control-plane/src/operator/oidc.rs:113`). That is a different
  route, a different realm, and a different question; it is not a precedent for
  reading `azp` here.
- **A realm is never parsed out of an issuer string** (ADR 0015). §2 binds an
  issuer to a tenant through a *registration*, which is a configured fact, not
  through string surgery on a URL.
- **Operator identity remains a separate mechanism** (ADR 0009). Nothing in a
  client realm can mint platform authority, and a client's redirect strategy is
  not an operator-facing capability.

## Consequences

**The runtime's posture becomes checkable.** Today "trusted ingress" is a
configuration value with no counterpart. After this, the counterpart exists in
`saas-fabric-platform` as a named route with a named claim set, and that
repository's `check.py` can fail when a route forwards `/v1/data` with nothing
in front of it. The runtime still cannot check its own exposure — that
limitation is ADR 0002's and survives — but the deployment can.

**The platform PR implementing §1 is a hard M2 dependency, and it has no owner
in this repository.** M2's acceptance is that a wrong issuer, a wrong audience,
an expiry, or direct-to-runtime access is refused *before* Fabric. Nothing
planned in this repository can meet that. §G is the checklist that PR
implements; who writes it, and when, is a question this ADR raises and cannot
answer.

**A cross-tenant token stops working, and some deployments stop starting.** §2
makes the issuer registry required configuration on the Data API path: a runtime
with no registrations refuses to start, at `build_identity`, which is step 1 of
the application graph. That is a deliberate fail-closed change and it means
every deployment must supply the registry before upgrading. It is also what
turns "the tenant claim is trustworthy because the edge said so" from an
assumption into a check.

**The edge and the runtime must agree about skew, and now say so.** Edge ≤ 30 s,
because 30 is the smallest downstream allowance. A gateway configured with a
five-minute allowance would accept tokens the front door then refuses, and the
symptom would be intermittent `401`s that look like an outage.

**One audience string, deployment-wide.** The Data API's audience and every
`IssuerRegistration.audience` must be equal, because a client carries one
mapper. A deployment that has already chosen a different audience for
`/v1/check` has to change one of them, and the safe order is the same as the
mapper's: change the registration, sweep, then enable the edge check.

**The realm needs an audience mapper before the edge can require `aud`.** The
mapper is written by reconciliation, so the ordering is: deploy the control
plane carrying this model → sweep → *then* turn on the edge's `aud` check.
Turning it on first refuses every genuine token, and ADR 0010 records that the
failure "presents as a signature problem rather than a missing mapper" — which
is a very expensive hour.

**Every declared client is stricter than it was.** S256 becomes mandatory for
clients that never had it, applied by the next reconciliation sweep, for `v1`
and `v2` documents alike. A client already performing PKCE is unaffected; a
client that was not is broken until it does, deliberately. **Client teams must
be notified before the sweep, not after** — this is the migrate/notify
obligation, and it belongs in a runbook in `saas-fabric-platform` beside the
deployment.

**No document break, and no bulk migration.** `v1` keeps parsing, so
`saas-fabric-clients` does not have to be migrated before the control plane is
deployed. The ordering risk that dominated the first draft of this decision is
gone. What replaces it is a slower, quieter change: **the repository migrates
document by document as operators edit them**, so a repository will hold both
versions for as long as some clients go unedited, and the console's version
badge is how anyone knows which is which. That is the cost of not migrating on a
schedule the platform controls, and it is the cost `schema.rs:6-11` chose.

**A client's document now says what kind of application it is.** That is the
durable win: `strategy: development` is a fact an operator can be shown, an
audit can read, and a future policy can act on — "no development client in a
production realm" becomes expressible, where today it is not.

**Some redirect URIs that parse today stop parsing.** Three loopback near-misses
(§3) and, over `https`, any IPv6 literal spelling of loopback. A client
document holding one becomes unreadable — `500 desired_state_invalid` — rather
than silently reclassified, which is the right direction and is still an
operator's morning. The shipped examples hold none of them; a survey of
`saas-fabric-clients` before the sweep is a cheap way to be sure.

**Reconciliation now sees a redirect URI it previously could not.** An
unparseable URI added out of band becomes drift and is overwritten on the next
sweep. The cost is that an operator who deliberately added a URI the model
cannot express will find it removed, with a plan that says a count rather than
naming it — and the answer is that the model should be widened, not that the
sweep should look away.

**Files that had to be split, not grown.** The gate measures *production* lines
— a trailing inline `#[cfg(test)]` module is subtracted
(`scripts/check_file_sizes.py:83-109`) — with a warning at 120 and a failure at
150 (`:36-37`). None of the three files this decision lands in had room for it,
and none is the cohesive wire-format type an exemption is for, so each grew a
module rather than a hundred lines. Measured on the commit this paragraph ships
in: `crates/fabric-keycloak/src/provider/mutate.rs` is **132**, in the "needs a
clear reason" band, and `declaration` is where the whole
strategy-to-representation mapping lands;
`crates/fabric-client-model/src/identity/redirect_uri.rs` is **138**, one
newtype with its impls, every rule it applies living in a module of its own;
`.../redirect_uri/authority.rs` is **148**, six short functions over one string.
What moved out is the classification itself — `kind.rs` for the scheme,
`host_kind.rs` for the host, `characters.rs` for the wildcards,
`private_use_scheme.rs` for RFC 8252 §7.1 — and, under `host_kind`, the four
files the host rule is made of: `ip_literal.rs`, `special_use.rs`,
`registered_domain.rs` and `registered_domain/label.rs`.

**Two documents in this repository said something this decision makes false**,
and both were corrected with it: `fabric-identity`'s crate documentation, which
said the tenant "comes from one place and one place only — the `tenant_id`
claim inside the bearer token" and now records that sentence as the cross-tenant
hole this closes (`crates/fabric-identity/src/lib.rs:16-34`), and
`authority.rs`'s note that an IPv6 literal's parsing ambiguity does not matter,
which now carries the bracket-aware rule that replaced it
(`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:42-52`).
Prose nobody checks is prose that drifts.

**The console shows more, and can still change nothing.** `ApplicationClients`
gains a strategy badge, a PKCE line and the document's schema version. Editing
an application client remains out of scope, so an operator reads the new fields
and edits the document through Git — the same asymmetry that already exists.

## What this does not decide

**Any Kubernetes or gateway resource.** No `Gateway`, `HTTPRoute`,
`SecurityPolicy`, ext-auth filter or `NetworkPolicy` is written here. §1 is a
contract the platform repository implements, and it is not satisfied by this ADR
merging.

**`webOrigins`.** Deliberately out of this slice. Nothing in M2's acceptance
needs it, and it is one more field Fabric would own and therefore reset on every
sweep if an operator set it by hand. **The consequence is accepted for now:** a
public browser client whose token request comes from a different origin fails
CORS, with nothing in Fabric to explain it, until a later slice derives origins
from the declared URIs.

**Confidential clients.** Synthesis Cloud's BFF is a confidential OAuth client
in the ordinary sense, and this model cannot declare one. In M2 the mobile
application is the public client and the BFF is a resource server behind the
same edge. If the BFF ever needs client credentials of its own, that is
ADR 0008's undesigned secret delivery and
[ADR 0017](0017-fabric-decides-which-client-secret-boundary-an-operation-reaches.md)'s
boundary question — a decision of its own, not an extension of this one.

**Whether the issuer→tenant binding moves into the tenants document.** §1 names
ADR 0018's published tenant binding as its eventual single source. Making that
move is a schema change to a document that rejects unknown fields at every
level, with a revision counter and a publication order of its own; it is a
decision of its own.

**How the custom-scheme variant matches.** Whether `scheme:/path` or
`scheme://host/path` is canonical, how a scheme is proven to belong to the
application declaring it, and whether the platform constrains schemes to a
reverse-DNS form derived from a declared host — all Lane E phase 2.

**Whether the runtime should verify signatures.** ADR 0002 answered that and
this ADR strengthens its premise rather than reopening it. `ValidatingReader`
remains available and remains opt-in, and §2's binding is deliberately
independent of which reader a deployment runs.

**Refresh tokens, session lifetimes, logout semantics, or MFA.** All identity
platform concerns (§8), and none of them becomes a Fabric concern by virtue of
Fabric declaring a client.

**Deletion.** Removing an application client from a document still deletes
nothing in Keycloak. ADR 0008's deletion question is untouched. Note that a
redirect URI narrowed in the document *is* now corrected, because the declared
set is written whole — it is the client itself that survives.

## §G. The platform-repo checklist

Everything §1 requires, as artefacts `saas-fabric-platform` produces. This list
is intended to be complete enough to implement without further analysis from
this repository. The row-by-row evidence expected for each is in
[the test matrix](../architecture/identity-edge-test-matrix.md) §G.

| # | Obligation | Concrete value |
|---|---|---|
| G1 | **Route.** One gateway route per tenant runtime service, terminating `/v1/data/*`, with the JWT policy below attached. One route serves **many tenants**; there is no per-tenant route. No other route reaches the runtime's protected path | path prefix `/v1/data` (`crates/fabric-data-api/src/routes.rs:21`); `/health` and `/ready` (`crates/fabric-api/src/health/routes.rs:15-16`) stay outside it |
| G2 | **JWKS.** Per-issuer JWKS URI, fetched and refreshed by the gateway, reachable from the gateway's own network position — not necessarily the public issuer URL | one per registered issuer |
| G3 | **Algorithm allow-list, per issuer.** Never read from the token header. `none` and every HMAC algorithm refused | e.g. `["RS256"]` per issuer; **not** a single global list, and **not** `fabric-identity`'s private `RS256/384/512` |
| G4 | **Issuer allow-list = the registered issuer set.** Exact-match `iss` against the set of issuers registered for the tenants this route serves. The edge decides membership only; the **runtime's registry decides the tenant** (§2) | exact strings, no prefixes, no patterns |
| G4a | **One generator, two artefacts.** The gateway's allow-list and the runtime's `[identity].trusted_issuers` are generated from the same tenant list, in the same change. Two hand-maintained lists is the shape a wrong tenant binding arrives in | see §1's drift table; the dangerous drift is invisible to the gateway |
| G5 | **Audience string.** One per deployment, required on every token, and **equal to every `IssuerRegistration.audience`** in the same deployment — a client carries one mapper, so unequal values make one route refuse every genuine token | e.g. `saas-fabric-data-api`. **Depends on G5a** |
| G5a | **Audience mapper.** Written by Fabric's reconciliation (`oidc-audience-mapper`, included custom audience = G5) onto every declared public client. The edge's `aud` check is enabled only after the first successful sweep | ordering is load-bearing (see Consequences) |
| G6 | **`exp` required, `nbf` checked.** A token with no `exp` is refused at the edge. The runtime will not refuse it | `crates/fabric-identity/src/readers/expiry.rs:36-38` |
| G7 | **Clock skew ≤ 30 s** | must satisfy edge ≤ min(60 s `fabric-identity`, 30 s `fabric-fga-auth`) |
| G8 | **Failure classes.** Credential → `401`; JWKS unreachable, refresh failed with no usable cached key, or refresh suppressed by cooldown → **`503`**; bad configuration → refuse to serve the route. Unknown `kid` is `401` only against a fresh successful snapshot | ADR 0016's table and rotation rules, `docs/decisions/0016-…:177-202` |
| G9 | **Header stripping.** `X-Tenant-Id`; the whole `x-jwt-claim-*` prefix; `x-forwarded-user`; `x-auth-request-*`; the operator-plane identity header. Prefix strip, not an enumerated list | applied before forwarding, on every request |
| G9a | **No claim projection.** The gateway MUST NOT write any verified claim into a request header — no `claim_to_headers` mapping, no `x-jwt-claim-*` emission, no header of the deployment's own naming. `Authorization` is the only thing that crosses the boundary | distinct from G9: G9 defends against the caller, G9a against the gateway's own configuration |
| G10 | **Forward `Authorization` verbatim.** No rewrite, no re-mint, no re-sign | byte-for-byte |
| G11 | **NetworkPolicy.** Runtime pods accept ingress only from the gateway's namespace (or the equivalent §9 control). Verified by a negative test from a scratch pod | `docs/architecture/tenant-runtime-data-api.md:324-332` |
| G12 | **`401` shape.** `WWW-Authenticate: Bearer error="invalid_token"`, no body naming the failed check, never a `302` to a login page | distinct from the runtime's `401`, which carries a cause and no challenge |
| G13 | **`503` shape.** A `Retry-After`, and a body that does not describe the credential. Never collapsed into `401` | a legitimate caller must not be told their token is bad |
| G14 | **The M2 acceptance run.** Two real Keycloak realm users complete authorization-code with S256 PKCE against a deployed runtime behind this edge | recorded, both users, plus the two Keycloak PKCE refusals observed |
| G15 | **An intercepted code cannot be redeemed without the verifier.** The property PKCE exists for, demonstrated rather than assumed | a mismatched `code_verifier` at the token endpoint |
| G16 | **Keycloak's real behaviour, recorded** — answered on 26.0.8 on 2026-09-06 (`docs/verification.md`, the probe findings; G17 re-verifies on 26.7.2). Any-port loopback; whether `GET /clients` returns `protocolMappers` and `attributes`; whether `PUT /clients/{id}` updates mappers or the sub-resource is required. Each amends §3 or §6 with evidence if it contradicts them | `docs/verification.md`, beside the 2026-08-28 findings (`docs/verification.md:497-534`) |
| G17 | **The version caveat, closed.** §3's any-port rule and §6's mapper behaviour were observed on Keycloak **26.0.8**, the image `scripts/e2e-services.sh` uses. LucentRoot runs **26.7.2**. The platform lane re-runs the same probe there and records the result beside the 26.0.8 findings | `docs/verification.md`. A difference amends §3 or §6 with the evidence, the way 26.0.8's observation amended the `Development` row |
