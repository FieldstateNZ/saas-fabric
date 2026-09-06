# Test matrix — edge trust, the tenant binding, and the native-client contract

The row-by-row evidence for
[ADR 0019](../decisions/0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md).
Every row is a refusal, or a deliberate non-refusal named so it reads as
intended rather than as a gap.

This document is **both repositories' checklist**. Sections A–F are what this
repository proves or cannot prove; section G is what `saas-fabric-platform`
owes, with the artefact expected for each. A row nobody can point at evidence
for is a row that has not been decided, only written down.

## How to read a row

| Column | Meaning |
|---|---|
| **Refused by** | The layer that says no. Not "where a test lives" — where the decision is made |
| **Provability** | One of: **existing test** (already in the tree, named); **proposed test** (this lane writes it); **platform obligation** (`saas-fabric-platform`, not provable here at all); **verified in slice 4 against a real Keycloak** (a socket-level fake cannot answer it, because the fake returns whatever the test hands it) |
| **Expected** | The status, the machine code, and what the caller is told — or, for a non-refusal, what happens instead |

Four layers appear:

- **Edge** — the ingress in `saas-fabric-platform`. Not provable in this
  repository at all; each row names its §G artefact.
- **Runtime** — `fabric-identity` and the tenant runtime plane, provable
  in-process through the real router.
- **Model / Keycloak** — `fabric-client-model`, `fabric-reconciliation` and
  `fabric-keycloak`, provable in-process against the socket-level fake, except
  where the fake is the thing being doubted.
- **Control plane** — `fabric-control-plane`'s API, provable through its router.

Test names are proposals in this repository's voice until the test is written.
A row marked **Existing test** names one that is in the tree at the commit this
document ships in, with the line it is on; a row marked **Proposed test** names
one that is not there yet.

**Scope note.** Section A governs the **tenant runtime Data API path**,
`/v1/data/*` (`crates/fabric-data-api/src/routes.rs:21`). It does not govern
`fabric-fga-auth`'s `/v1/check`, which verifies signature, `iss`, `aud`, `exp`,
`nbf` and a per-issuer algorithm allow-list for itself
(`crates/fabric-fga-auth/src/verifier.rs:55-92`), nor the control plane's
operator routes, which verify for themselves under ADR 0010. Each is
authoritative on its own route.

---

## A. Token claims the edge is the only thing that checks

On this path the canonical reader decodes the payload and checks the validity
window, and nothing else
(`crates/fabric-identity/src/readers/trusted_ingress.rs:92-99`; ADR 0002's table
at `docs/decisions/0002-…:70-79`). Saying so plainly is the point of this
section: a reader must not come away thinking Fabric catches these.

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| A1 | **Issuer outside the registered set** — token signed by a realm the route does not serve | Edge (and, for the tenant it names, the runtime — see A11) | **Platform obligation** §G4: exact `iss` match against the registered issuer set, no prefixes. The runtime half *is* provable here — A11 | `401`, `WWW-Authenticate: Bearer error="invalid_token"`, no body naming the check |
| A2 | **Wrong audience** — valid token minted for another API | Edge only | **Platform obligation** §G5, and it only works once §G5a's mapper is written — see A13 | `401`, as above |
| A3 | **Unsigned token / `alg: none`** | Edge only | **Platform obligation** §G3. Not provable here in the other direction either: the canonical reader decodes the payload without inspecting the header, which is why `encode_unsigned_token` is how its own tests mint tokens (`crates/fabric-identity/src/readers/trusted_ingress.rs:112`) | `401` |
| A4 | **Signature from an unknown key** (`kid` not in the realm JWKS) | Edge only | **Platform obligation** §G8 | `401` **only** against a fresh successful snapshot; otherwise `503` — see A12 |
| A5 | **`alg` confusion** — HS256 signed with the RSA public key | Edge only | **Platform obligation** §G3. The allow-list is **per issuer**, pinned in the gateway, never read from the token header. It is **not** a single global constant, and it is **not** `fabric-identity`'s private `RS256/384/512` (`crates/fabric-identity/src/readers/validation_rules.rs:8`), which belongs to the defence-in-depth posture and is not referenceable by the platform. The shape to copy is `IssuerRegistration.algorithms` (`crates/fabric-fga-auth/src/registry/registration.rs:51-57`) | `401` |
| A6 | **Expired token** | Edge **and** runtime | **Existing test**, the runtime half — `an_expired_token_is_rejected_even_in_the_trusted_ingress_posture` **(exists**, `crates/fabric-data-api/tests/identity_boundary.rs:85`**)**; `rejects_an_expired_token_even_though_it_does_not_verify_signatures` **(exists**, `crates/fabric-identity/src/readers/trusted_ingress.rs:142`**)** | Edge `401`; runtime `401` `bearer token has expired` |
| A7 | **Token not yet valid (`nbf`)** | Edge **and** runtime | **Existing test** — `rejects_a_token_minted_for_later_use` **(exists**, `.../trusted_ingress.rs:152`**)** | `401` `bearer token is not yet valid` |
| A8 | **Fractional `exp`/`nbf`** (RFC 7519 §2 legal; previously switched the check off) | Runtime | **Existing tests** — `rejects_an_expired_token_with_a_fractional_expiry` (`.../trusted_ingress.rs:183`), `rejects_a_token_minted_for_later_use_with_a_fractional_not_before` (`:171`) | `401` |
| A9 | **Clock skew: three hops, one inequality** — a token 45 s past `exp` | Whichever hop is strictest | **Existing test** for the runtime ceiling — `a_widened_window_still_cannot_be_widened_past_the_ceiling` (`.../trusted_ingress.rs:194`) — plus a **proposed test**, `the_edge_leeway_ceiling_is_the_smallest_downstream_allowance`, asserting `CLOCK_SKEW_TOLERANCE_SECONDS` (30, `crates/fabric-fga-auth/src/verifier.rs:25`) `<= LeewaySeconds::DEFAULT` (60, `crates/fabric-identity/src/readers/leeway.rs:65`), so a future narrowing of either is caught here. The **edge's own value is a platform obligation** (§G7, ≤ 30 s) | Edge `401` if the edge is strictest; `fabric-fga-auth` `401` at 30 s on `/v1/check`; runtime `401` at 60 s on the Data API. A gateway configured looser than 30 s produces intermittent `401`s downstream that look like an outage |
| A10 | **Direct-to-runtime access** — a pod inside the cluster calling `/v1/data/*` without passing the gateway | Network, not code | **Platform obligation** §G1 + §G11, and **it cannot be anything else**: "the runtime cannot detect its own exposure" (`docs/decisions/0002-…:139-141`). Both artefacts, because a route is skippable from inside the cluster | Connection refused / reset at the network layer — **not** an HTTP status. Negative test from a scratch pod in the cluster |
| A11 | **Unregistered issuer** — a genuinely signed token whose `iss` is in no registration | **Runtime** (and the edge, independently) | **Proposed test** — `a_token_from_an_unregistered_issuer_is_refused` in `crates/fabric-identity/src/resolver_tests.rs`, plus `a_token_from_an_unregistered_issuer_is_refused_at_the_data_api` in `crates/fabric-data-api/tests/identity_boundary.rs` with `query_count() == 0` | `401`, credential class. Message names the shape, never the issuer value — it is attacker-controlled |
| A11a | **No `iss` at all** | **Runtime** | **Proposed test** — `a_token_with_no_issuer_is_refused_rather_than_treated_as_unregistered`. Closes on this path the hole ADR 0002 records in the allowlists, where "any token that simply omitted `iss` sailed past an issuer allowlist — a security control that silently did nothing" (`docs/decisions/0002-…:96-101`; `crates/fabric-identity/src/readers/allowlists.rs:5-21`) | `401`, credential class |
| A12 | **JWKS unreachable, or refresh suppressed by the cooldown, with no usable cached key** | Edge | **Platform obligation** §G8. The rule to implement is ADR 0016's, not the gateway's default: unknown `kid` is `401` **only** against a fresh successful snapshot; otherwise `503` (`docs/decisions/0016-…:185-202`; implemented for `/v1/check` at `crates/fabric-fga-auth/src/cache.rs:92-142`). Two windows, not one — the cooldown must never decide an authentication result | **`503`**, with `Retry-After`, and a body that does not describe the credential. **Never `401`**: a Keycloak outage must not tell a legitimate user their credentials are wrong |
| A13 | **The audience mapper is missing, so no genuine token carries the required `aud`** | Edge (it refuses everything) | **Split.** That the mapper is **written** is a **proposed test** — `a_declared_client_is_written_with_the_platform_audience_mapper` in `crates/fabric-keycloak/tests/keycloak_adapter.rs`, asserting the POST body contains `"oidc-audience-mapper"` and `"included.custom.audience":"<configured>"`. That Keycloak **returns** it on `GET /clients`, and that `PUT /clients/{id}` **updates** it rather than requiring `/clients/{id}/protocol-mappers/models`, is **verified in slice 4 against a real Keycloak** — the echoing fake would confirm either answer. The *ordering* is a **platform obligation** (§G5a) | If the mapper is absent, the edge refuses **every** genuine token with `401`, and ADR 0010 records that it "presents as a signature problem rather than a missing mapper" (`docs/decisions/0010-…:70-74`) |
| A13a | **The Data API audience and `IssuerRegistration.audience` differ** | Whichever route the client's single mapper does not satisfy | **Proposed test** — `the_data_api_audience_and_the_front_doors_audience_are_one_string`, asserting the composition root refuses a configuration in which they differ. A client carries exactly one audience mapper; `fabric-fga-auth` puts its registration's audience straight into `set_audience` and requires the claim (`crates/fabric-fga-auth/src/verifier.rs:79-80`) | Startup failure naming both settings. Left unchecked: one route refuses **every** genuine token, presenting as a signature problem |
| A14 | **Missing `exp`** — a token that never expires | **Edge only** | **Proposed test** for the runtime's *acceptance* — `a_token_with_no_expiry_is_accepted_because_the_edge_required_one` in `crates/fabric-identity/src/readers/expiry.rs`'s test module, asserting `Ok`, with a comment naming §G6. Pins the *absence* of a check, so nobody later reads it as an oversight or assumes a backstop | Edge `401`. Runtime: **not a refusal** — `expiry::ensure_not_expired` returns `Ok` when the claim is absent (`crates/fabric-identity/src/readers/expiry.rs:36-38`), deliberately, per ADR 0002 (`docs/decisions/0002-…:88-92`) |
| A15 | **The gateway is configured to project a claim into a header** — e.g. `claim_to_headers` writing `x-jwt-claim-tenant-id` | Nothing, if it is configured | **Platform obligation** §G9a. Distinct from §G9 and from B7: this is the gateway adding a header, not a caller sending one, so no strip catches it and the runtime never sees anything wrong | Configuration review, and a gateway test asserting the forwarded request carries `Authorization` and no `x-*` claim header. The failure mode is silent: a second identity source arriving on the **trusted** side of the boundary |

**Companion test for A1/A2/A3 honesty:** a **proposed test**,
`the_canonical_posture_does_not_examine_the_signature_or_the_audience` in
`crates/fabric-identity/src/readers/trusted_ingress.rs` — a token carrying
`aud: somebody-else` and an unverifiable signature is *accepted by the reader*,
with a comment naming the edge as the thing that refuses it. Note this is the
**reader**, not the resolver: after §2 the resolver refuses an unregistered
`iss`, so the pair of tests together say exactly which layer does what.

---

## B. Identity source — the issuer names the tenant, and nothing else does

**One choke point for the whole section.** Every Data API integration suite
builds its tokens through `token_for` in
`crates/fabric-data-api/tests/support/requests.rs:10-16` — fifteen suites, all
of them, because all fifteen import `request` or `json_request`. Adding an `iss`
from the test registry there is one function, not fifteen files. The one
request built by hand rather than through the helper
(`identity_boundary.rs:23-32`) is the tenant-header case, which is refused
before the token is read (`crates/fabric-identity/src/resolver.rs:45`) and
therefore needs no issuer.

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| B1 | **`X-Tenant-Id` present** | Runtime (and stripped at the edge) | **Existing test** — `a_tenant_header_is_rejected_outright` (`crates/fabric-data-api/tests/identity_boundary.rs:14`) | `400`, `tenant selection through the x-tenant-id header is not permitted`, and `query_count() == 0` |
| B1a | **The refusal is the configured default** | Configuration | **Existing test** — `rejecting_the_tenant_header_is_the_default_posture` (`crates/fabric-identity/src/config.rs:101`). ADR 0019 §7 keeps `true` as the shipped default and the recommended posture, and deliberately does **not** require it of every deployment: §G9 strips the header, so the switch decides only what a request that never arrives would have been told | Not a refusal; a pinned default. The invariant that *is* required is that no code path reads the header as a tenant source (`crates/fabric-identity/src/resolver.rs:78-80`) |
| B2 | **No `Authorization` header** | Runtime | **Existing test** — `a_request_with_no_token_is_rejected` (`identity_boundary.rs:41`) | `401` `request has no Authorization header` |
| B3 | **Not a bearer scheme** | Runtime | **Existing tests** — `a_basic_credential_is_rejected` (`crates/fabric-identity/src/bearer.rs:70`), `an_empty_bearer_credential_is_rejected` (`:78`) | `401` `Authorization header is not a Bearer token` |
| B4 | **No tenant claim** | Runtime | **Existing test** — `a_token_with_no_tenant_claim_is_rejected` (`identity_boundary.rs:59`). **Keeps its meaning under §2**: the claim stays required. Its token gains an `iss` from `token_for`, so it now exercises "registered issuer, no claim" rather than "no issuer, no claim" — which is the case the rule is actually about | `401` `bearer token has no tenant_id claim` (`crates/fabric-identity/src/errors.rs:49-56`) |
| B5 | **Tenant claim that is not a valid identifier** (`Acme Corp`) | Runtime | **Existing test** — `a_token_with_an_invalid_tenant_claim_is_rejected` (`identity_boundary.rs:72`) | `401` `the tenant_id claim is not a valid tenant identifier`, value not echoed |
| B6 | **Non-string tenant claim** (`"tenant_id": 42`) | Runtime | **Existing test** — `a_non_string_tenant_claim_reads_as_absent_rather_than_being_coerced` (`crates/fabric-identity/src/claims.rs:123`) | `401` — reads as absent via `TokenClaims::string` (`crates/fabric-identity/src/claims.rs:32`), so it lands on B4's message |
| B7 | **A claim-projection header from a caller** — `x-jwt-claim-tenant-id: globex` alongside a token for `acme` | Edge strips it; runtime ignores it | **Proposed test**, the runtime half — `a_claim_projection_header_changes_nothing_about_the_tenant`: the same request twice, with and without the header, both resolve `acme` and the connector sees the same predicate | **Not a refusal**: the header is inert. The edge strip is a **platform obligation** (§G9, a **prefix** strip, not an enumerated list). The gateway *emitting* one is A15 / §G9a |
| B8 | **`azp` / `client_id` claim naming another client, on the Data API path** | Neither reads it, **on this path** | **Proposed test** — `the_client_identity_claim_is_not_an_identity_source_on_the_data_api_path`: a token with `azp: globex-mobile`, `iss` registered to `acme` and `tenant_id: acme` resolves `acme`. Scoped in the name, because ADR 0010's operator plane **does** gate on `azp` (`crates/fabric-control-plane/src/operator/oidc.rs:113`), and an unqualified name would read as forbidding that | **Not a refusal**: the claim is not read here. The named exception is the operator route |
| B8a | **`azp` on the operator route is still the gate** | Control plane | **Existing test** — `refuses_a_token_issued_to_another_client_in_the_same_realm` (`crates/fabric-control-plane/src/operator/oidc/oidc_tests.rs:130`). Listed so B8's scoping is provable in both directions | `OperatorAuthError::NotAnOperator` |
| B9 | **Issuer → tenant mismatch** — `iss` registered to `acme`, `tenant_id` claim says `globex` | **Runtime** | **Proposed tests** — `a_tenant_claim_that_disagrees_with_its_issuer_is_refused` in `crates/fabric-identity/src/resolver_tests.rs`, plus the composed `a_token_cannot_name_a_tenant_its_issuer_does_not_own` in `crates/fabric-data-api/tests/identity_boundary.rs` asserting `query_count() == 0` | `401`, credential class. Neither value echoed. **The tenant is `acme` or the request is refused; it is never `globex`** |
| B10 | **Issuer registered, tenant claim agrees** | Nothing — the ordinary case | **Proposed test** — `a_tenant_that_agrees_with_its_issuer_is_the_tenant_that_is_used`, asserting the resolved tenant comes from the **registration** and not from the claim (change the claim's spelling to a different-but-equal-looking value and it is a mismatch, not a rename) | **Not a refusal.** The registration's tenant. This row exists so the pair B4/B9 cannot be read as "the claim is optional": it is required, it must agree, and it is never the source |
| B11 | **Empty issuer registry** | Configuration, at startup | **Proposed tests** — `a_runtime_with_no_trusted_issuers_refuses_to_start` in `crates/fabric-identity/src/config.rs`, and the composition-level `the_example_configuration_builds_an_identity_resolver`, which reaches it through `build_identity` (`crates/fabric-identity/src/registration.rs:23`). Follows ADR 0016's configuration class (`crates/fabric-fga-auth/src/errors.rs:19-23`) | Startup failure naming `identity.trusted_issuers`. Never a per-request failure. Note the enforcement point is `build_identity`, reached from `crates/fabric-api/src/startup/application.rs:44-47` — **not** `AppConfig::validate`, so `crates/fabric-api/tests/example_configuration.rs` does not catch it; `crates/fabric-api/tests/composed_surface.rs:90-94` does |
| B12 | **Two registrations naming the same issuer** | Configuration, at startup | **Proposed test** — `two_registrations_for_one_issuer_are_refused_at_startup`. Which one wins would depend on map ordering, which is ADR 0016's stated reason (`crates/fabric-fga-auth/src/errors.rs:25-32`) | Startup failure naming the issuer |
| B13 | **Registry drift: the gateway admits an issuer the runtime does not know** | **Runtime** | **Proposed test** — the A11 pair. This is the drift row read from the gateway's side, and it is here to record that it **fails closed** | `401` at the runtime. The token reached the boundary and was refused inside it |
| B14 | **Registry drift: the runtime knows an issuer the gateway does not admit** | **Edge** | **Platform obligation** §G4 / §G4a. Not provable here — the runtime never sees the request | `401` at the edge. The token never arrives. Also fails closed |
| B15 | **Registry drift: the runtime binds an issuer to the wrong tenant** | **Nothing** | **Neither.** No allow-list catches it: the gateway has no opinion about tenants, and the runtime is the authority. **The mitigation is generation, not validation** — §G4a requires both artefacts to come from one tenant list in one change. Listed because a matrix that omitted it would imply the two allow-lists cover each other | The wrong tenant's data, with no error anywhere. This is the reason ADR 0019 §1 names ADR 0018's published tenant binding as the eventual single source |
| B16 | **A `ValidatingReader` deployment whose `[token].issuers` and `[identity].trusted_issuers` differ** | Configuration, at startup | **Proposed test** — `a_validating_deployment_must_name_the_same_issuers_twice` in `crates/fabric-api/src/config/validation_tests.rs`. The check belongs in `AppConfig::validate`, which exists for "relationships *between* settings, and between settings owned by different crates" (`crates/fabric-api/src/config/validation.rs:8-13`) and runs at `crates/fabric-api/src/main.rs:42`, before the application is built | Startup failure naming both settings. Left unchecked: a token that verifies and cannot be placed, or a tenant binding for an issuer whose signature nobody will accept |

---

## C. PKCE

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| C1 | **`pkce: plain` in a document** | Model, at deserialisation | **Proposed test** — `a_plain_pkce_method_is_not_a_value_this_model_can_hold` in `identity/pkce_method.rs` | `DesiredStateError::Malformed`, serde naming `spec.identity.clients[0].pkce` and `unknown variant plain, expected s256` |
| C2 | **`pkce: plain` submitted through the API** | Control plane | **Proposed test** — `a_plain_pkce_method_is_refused_at_the_api` in `crates/fabric-control-plane/tests/control_plane_api.rs` | `400`, code `invalid_request` |
| C3 | **`pkce` omitted from a `v2` document** | Model, as a missing field | **Proposed test** — `a_v2_client_must_state_its_pkce_method`. There is no default: `ClientProtocol`'s precedent (`crates/fabric-client-model/src/identity/oidc_client.rs:5-16`) is followed, not contradicted — a defaulted field is a meaning a document acquires without saying it | `DesiredStateError::Malformed` (serde `missing field pkce`); `400 invalid_request` on write, `500 desired_state_invalid` on read of a stored one |
| C4 | **The Keycloak write body omits the PKCE attribute** | The wire test | **Proposed test** — `a_declared_client_is_written_with_the_s256_challenge_method` in `crates/fabric-keycloak/tests/keycloak_adapter.rs`, asserting the POST body contains `"pkce.code.challenge.method":"S256"`. **Mutation-proved** per `docs/delivery.md:68`: delete the attribute from `declaration()`, watch this fail, restore | Test failure, not a runtime refusal |
| C5 | **Keycloak holds a declared client with no PKCE attribute** (created before this slice, or edited by hand) | Reconciliation, as drift | **Proposed test** — `a_client_without_the_challenge_method_does_not_match_its_declaration` in `crates/fabric-reconciliation/src/plan/diff_tests.rs` | Plan contains `UpdateOidcClient`; status becomes `drifted` then `applied` |
| C6 | **Keycloak holds `plain`** | Reconciliation, as drift | **Proposed test** — `a_client_downgraded_to_plain_is_corrected`. `ObservedOidcClient.challenge_method` is `Option<PkceMethod>`, so an unparseable value reads as `None`, which is not `Some(S256)` — no `Plain` variant is needed anywhere in the model | `UpdateOidcClient` |
| C7 | **Real authorization request with no `code_challenge`** | Keycloak, at the authorization endpoint | **Platform obligation** §G14 — the M2 acceptance run. Keycloak enforces it once the attribute is set; nothing in this repository can prove that | `400 invalid_request`, `Missing parameter: code_challenge` |
| C8 | **Real code exchange with a mismatched `code_verifier`** | Keycloak, at the token endpoint | **Platform obligation** §G15 — same run. This is the property PKCE exists for | `400 invalid_grant`, `PKCE verification failed` |
| C9 | **The wire spelling drifts between the writer and the comparer** | Structural | **Proposed test** — `the_challenge_method_has_one_spelling`: the adapter's write and `diff::matches` both call `PkceMethod::as_wire_value()` in `fabric-client-model`, so a change to the spelling changes both | Test failure |

---

## D. Redirect strategy

`RedirectUriKind` is the partition ADR 0019 §3 states: **scheme first**
(private-use scheme → its own kind, whatever its authority; `http`/`https` →
classified by host; anything else refused), then **host** within `http`/`https`
(loopback → `.internal` → `Https`). Scheme and host are **lower-cased before
every test**.

The last arm is a **positive** rule, and the D17 rows below are what that
means. `Https` is not "everything my parser did not recognise as an address";
it is a registered domain — ASCII, at least two labels, each 1–63 characters of
letters, digits and hyphens with no hyphen on either end, 253 characters in
total at most, and a final label that is neither all-numeric nor `0x`-prefixed.
Every IP address literal is therefore refused under `https`, in every spelling,
and so is every bracketed authority that is not the loopback `[::1]`. So is
every name under a top-level domain the public DNS has reserved — `.local`
(RFC 6762), `.test`, `.example` and `.invalid` (RFC 2606) — because none of
them can be registered, and `.localhost` (RFC 6761 §6.3), which is a loopback
near-miss rather than an unregistrable one.

### D0 — normalisation

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| D0a | **`https://LOCALHOST:5173/cb` under `claimedHttps`** | Model validation, after lower-casing | **Existing test** — `an_upper_case_loopback_host_is_still_a_loopback_host` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:136`), with the classification half at `crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:116`. Before this slice the host was not examined at all under `https`, so this parsed and classified as `Https` | `InvalidField`, `400` on write / `500` on read. Named as `Loopback` under a strategy that admits only `Https` |
| D0b | **`https://ADMIN.CORP.INTERNAL/cb` under `claimedHttps`** | Model validation, after lower-casing | **Existing test** — `an_upper_case_internal_host_is_still_a_private_network_host` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:144`, and `crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:123`). Before this slice `.internal` was matched case-sensitively, so this classified as an ordinary public host | `InvalidField`, `400` / `500` |
| D0c | **`http://LOCALHOST:5173/cb` under `development`** | Nothing — permitted after lower-casing | **Existing test** — `an_upper_case_loopback_host_is_accepted_over_plain_http` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:109`). **Refused at `bc1f58c`**: `LOOPBACK.contains(&host)` was case-sensitive, so it was a `BadBoundary` | **Not a refusal.** A widening, and deliberate |
| D0d | **`HTTPS://www.example.com/cb`** | Nothing — permitted after lower-casing | **Existing test** — `an_upper_case_scheme_is_the_same_scheme` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:101`). **Refused at `bc1f58c`**: `strip_prefix("https://")` was case-sensitive | **Not a refusal** |

> The precedent for taking these four seriously is in this repository's own
> record: "a mixed-case fixture, absent, made a `to_lowercase()` bug invisible
> to both the socket test and the real engine" (`docs/delivery.md:61-62`).

### D1–D15 — classification and entitlement

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| D1 | **Loopback URI under `claimedHttps`** — `http://localhost:5173/callback` | Model validation | **Existing test** — `a_development_callback_is_refused_under_the_production_strategy` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:103`) | `DesiredStateError::InvalidField`, `spec.identity.clients`, naming the strategy, the URI's kind and what the strategy admits. `400` on write, `500` on read |
| D1a | **`https://` loopback under `claimedHttps`** — `https://localhost:5173/callback` | Model validation | **Existing test** — `a_loopback_host_is_not_a_claimed_https_callback_even_over_tls` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:115`). **The host rule's sharpest edge**: the scheme is right and the classification is still `Loopback` | `InvalidField`, `400` |
| D1b | **`https://` `.internal` under `claimedHttps`** — `https://admin.corp.internal/cb` | Model validation | **Existing test** — `a_private_network_host_is_not_a_claimed_https_callback_even_over_tls` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:126`) | `InvalidField`, `400` |
| D1c | **`https://localhost/cb` under `development`** | Nothing — permitted | **Existing test** — `a_loopback_callback_may_be_served_over_tls` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:152`). A developer running a local TLS proxy writes exactly this; it is `Loopback`, so `Development` is the strategy that holds it | **Not a refusal.** Named because "the scheme is https, therefore the strategy is claimedHttps" is the intuition the partition breaks |
| D2 | **`https://` public URI under `development`** | Model validation | **Existing test** — `a_public_callback_is_refused_under_the_development_strategy` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:162`). Refused rather than waved through as "stricter than needed": the strategy is a statement about what the client *is* | `InvalidField`, `400` |
| D3 | **`.internal` URI under `claimedHttps`** (plain HTTP) | Model validation | **Existing test** — `a_private_network_callback_is_refused_under_the_production_strategy` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:173`) | `InvalidField`, `400` |
| D4 | **`strategy: customScheme`** | Model validation, deferred | **Proposed tests** — `the_custom_scheme_strategy_is_refused_with_the_phase_that_will_carry_it`, asserting the message contains `Lane E phase 2`; and **both statuses, one test each**: `a_custom_scheme_client_is_refused_at_the_api` (write) and `a_stored_custom_scheme_client_makes_the_document_unreadable` (read) | Write: `400`, code `invalid_request`. Read: **`500`**, code `desired_state_invalid` (`crates/fabric-control-plane/src/errors/status_mapping.rs:80-82`, `codes.rs:16`) — the whole document, not the one client. Never coerced into another variant |
| D5 | **A private-use-scheme URI inside another strategy** — `nz.fieldstate.slipway:/cb` under `development` | Classification, then validation | **Existing test** — `a_private_use_scheme_is_not_a_loopback_redirect` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:183`) | `InvalidField`, `400` |
| D5a | **`nz.fieldstate.slipway://localhost/cb`** | Classification — it is `PrivateUseScheme`, not `Loopback` | **Existing tests** — `a_private_use_scheme_with_a_loopback_authority_is_still_a_private_use_scheme`, over the strategy (`crates/fabric-client-model/src/identity/client_rules_tests.rs:191`) and over the classification (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:351`). **This is the row scheme-first exists for.** A host-first partition would classify it by the `localhost` in its authority and hand a native application's callback the entitlement a development HTTP callback has. The authority of a private-use URI is not a network location; RFC 8252 §7.1's own examples put nothing there | Under `development`: `InvalidField`, `400` — the kind is not admitted. Under `customScheme`: refused as deferred (D4). Never `Loopback` |
| D6 | **Empty `uris` list** | Model validation | **Existing test** — `a_strategy_with_no_callback_could_never_sign_a_user_in` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:251`), which replaced `an_application_client_with_no_redirect_uri_is_refused` over `validation.rs:78-90` | `InvalidField`, `400` |
| D7 | **Wildcard in the host** — `https://*.example.com/callback` | `RedirectUri::try_new` | **Existing test** — `refuses_a_wildcard_in_the_host` (`crates/fabric-client-model/src/identity/redirect_uri.rs:160`). **Mutation-proved**, and now against **two** guards: `characters::check` refuses a `*` that is not the final character (`crates/fabric-client-model/src/identity/redirect_uri/characters.rs:68`), and the registered-domain rule will not take `*` as a label character. The test carries `http://*.lucentroot.internal/callback` as well, which reaches only the first — remove either guard and it still has to fail | `IdentifierError::DisallowedCharacter` → `Malformed`, `400` |
| D7a | **Trailing wildcard under `claimedHttps`** — `https://www.example.com/*` | Strategy validation | **Existing test** — `a_wildcard_callback_is_refused_under_the_production_strategy` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:199`). The URI parses (`crates/fabric-client-model/src/identity/redirect_uri.rs:144`) — the strategy is what refuses it, naming RFC 9700 §2.1's exact-matching requirement and that Universal/App Links need exact URLs | `InvalidField`, `400` |
| D7b | **Trailing wildcard under `privateNetwork`** | Strategy validation | **Existing test** — `a_wildcard_callback_is_refused_under_the_private_network_strategy` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:211`) | `InvalidField`, `400` |
| D7c | **Trailing wildcard under `development`** | Nothing — permitted | **Existing test** — `a_trailing_path_wildcard_is_the_one_place_a_development_callback_may_use_one` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:219`) | **Not a refusal.** The one permitted path wildcard |
| D8 | **`javascript:` scheme** | `kind::classify` (`crates/fabric-client-model/src/identity/redirect_uri/kind.rs:109`) | **Existing tests** — `refuses_a_javascript_scheme` (`crates/fabric-client-model/src/identity/redirect_uri.rs:173`) and `a_scheme_that_is_not_http_is_refused` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:82`), which covers `data:`, `file:` and `ftp://` as well. Must still fail *after* the private-use-scheme rule is added — this is the regression that rule most plausibly causes. **Mutation-proved** | **`Unadmitted`**, `400`, naming the schemes this model classifies. Not `BadBoundary`, which would tell the author of `javascript:alert(1)` that their value "must start and end with an alphanumeric character" — the same misdirection D9 and D10 record |
| D9 | **Userinfo in the authority** — `http://x.internal@evil.example.com/` | `authority::reject_userinfo` (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:100`) | **Existing test** — `a_public_host_cannot_smuggle_the_private_domain_through_userinfo` (`authority_tests.rs:61`), covering both schemes | **`Unadmitted`**, `400`. Not `BadBoundary`: the value has no bad character in it, and "must start and end with an alphanumeric character" sends its author hunting for a typo that is not there. The message says a redirect URI has no legitimate use for credentials |
| D10 | **Plain HTTP on a public host** — `http://www.example.com/callback` | `host_kind::classify`, the `!secure` arm (`crates/fabric-client-model/src/identity/redirect_uri/host_kind.rs:108`) | **Existing tests** — `refuses_plain_http_anywhere_else` (`crates/fabric-client-model/src/identity/redirect_uri.rs:155`), `plain_http_is_refused_on_a_public_host` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:45`), plus `plain_http_on_a_public_host_names_the_boundary_rather_than_a_typo` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:274`), which asserts the message | **`Unadmitted`**, `400`, naming the boundary: `https for a public host; plain http only on loopback or a .internal host`. Was `BadBoundary`, which is the same misdirection as D9 |
| D11 | **`.internal` as a path, not a host** — `http://evil.example.com/.internal` | `authority::of`, which keeps only what precedes the first `/`, `?` or `#` (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:38`), so `host_kind::classify` is handed a public host and nothing else | **Existing test** — `a_public_host_cannot_smuggle_the_private_domain_through_the_path` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:53`), over the path, query and fragment spellings | **`Unadmitted`**, `400` — D10's plain-HTTP message, because once the authority has been taken that is exactly what this is. Was `BadBoundary` |
| D12 | **Unregistered redirect at authorization time** — the app asks for a `redirect_uri` the realm does not hold | Keycloak | **Platform obligation** for the real behaviour (§G14). In-process analogue: a **proposed test**, `only_the_declared_callbacks_are_written`, asserting the POST body's `redirectUris` is exactly the declared set and contains nothing else | Keycloak: `400 invalid_redirect_uri`, and it does **not** redirect to the supplied URI |
| D13 | **An unparseable URI added to Keycloak by hand** — `http://evil.example.com/steal` | Reconciliation, as drift | **Proposed tests** — `a_redirect_uri_this_model_cannot_parse_is_drift` in `crates/fabric-reconciliation/src/plan/diff_tests.rs`, plus `an_unmodellable_redirect_uri_is_counted_rather_than_dropped` in `crates/fabric-keycloak/tests/keycloak_adapter.rs`. Today `observe::clients` drops it with `.filter_map(… .ok())` (`crates/fabric-keycloak/src/provider/observe.rs:75-79`), the surviving set equals the declared set, and `matches` reports converged (`crates/fabric-reconciliation/src/plan/diff.rs:76-80`). **Mutation-proved**: restore the silent drop, watch this fail | Plan contains `UpdateOidcClient`; the declared set is rewritten and the extra URI removed. The **count** is reported, never the value — it is attacker-influenced. The count lives on `ObservedOidcClient` (`crates/fabric-reconciliation/src/provider/observed.rs:36-47`), so the plan names the client to rewrite |
| D13a | **A parseable URI changed by hand** — `https://evil.example.com/callback` | Reconciliation, as drift | **Existing test** — `an_application_client_with_a_changed_redirect_uri_is_updated` (`crates/fabric-reconciliation/src/plan/diff_tests.rs:103-118`). Kept, and now clearly distinguished from D13: this one already worked because the URI parses | `UpdateOidcClient` |
| D14 | **Loopback with no port** — `http://127.0.0.1/callback` under `development` | Nothing — permitted, meaning any port | **Existing test** — `a_loopback_callback_with_no_port_admits_any_port` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:227`), against RFC 8252 §7.3 | **Not a refusal** |
| D14a | **Loopback with `*` in the port** — `http://127.0.0.1:*/callback` | `RedirectUri::try_new` (`crates/fabric-client-model/src/identity/redirect_uri/characters.rs:56`) | **Existing tests** — `a_wildcard_port_is_refused_because_a_portless_loopback_callback_already_matches_any_port` (`crates/fabric-client-model/src/identity/redirect_uri.rs:182`), over both spellings: `:*/callback` and the bare `:*`, which puts its `*` last and would otherwise pass as a trailing path wildcard; and `a_wildcard_port_is_refused_on_a_private_use_scheme_in_words_that_fit_one` (`crates/fabric-client-model/src/identity/redirect_uri.rs:200`), which is why the message is scheme-neutral. An earlier draft of this matrix had this row as **not a refusal**; the probe below is what changed it | **`Unadmitted`** → `Malformed`, `400`. The message leads with the fact that holds for every scheme — no identity provider matches a wildcard port — and then names what to write instead: over `http` a portless loopback callback already matches any port |
| D14b | **What any-port actually means, over each scheme** | Keycloak | **Observed on Keycloak 26.0.8, 2026-09-06** — one version, on one day, against the image `scripts/e2e-services.sh` uses, and that is the whole of what has been seen. LucentRoot runs **26.7.2** and nothing has been run against it: re-verifying there is an **open platform obligation** (§G17), not something already done. Over `http`, a loopback URI registered **without a port** matches the same path on any port — Keycloak compares no port for it, which is RFC 8252 §7.3's requirement met. Over `https`, and whenever a port is written under either scheme, the match is **exact**. `http://127.0.0.1:*/cb` matches nothing at all, which is why D14a is a refusal. The in-repo half is `a_loopback_callback_with_no_port_admits_any_port` (`crates/fabric-client-model/src/identity/client_rules_tests.rs:227`), which proves only that the portless spelling is admitted — the matching is Keycloak's | **Not a refusal.** The asymmetry is recorded rather than modelled: nothing in this repository can enforce what Keycloak compares, so the ADR states it and this row names where it came from |
| D14c | **What Keycloak is actually told about any-port** | Adapter | **Verified in slice 4 against a real Keycloak.** The socket fake returns whatever the test hands it, so it can confirm either answer and prove neither. If Keycloak cannot express any-port for loopback, ADR 0019 §3's `Development` row is amended in that slice **with the evidence** | Recorded behaviour, in `docs/verification.md` beside the 2026-08-28 findings (`docs/verification.md:487-524`) |
| D15 | **IPv6 loopback** — `http://[::1]:5173/callback` under `development` | Nothing — permitted | **Existing test** — `the_ipv6_loopback_address_is_a_development_callback` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:139`). Not accepted at `bc1f58c`: `LOOPBACK` held `["localhost", "127.0.0.1"]` and an IPv6 literal was refused over plain HTTP by design. It required **bracket-aware host parsing** — `host` split on the first colon, turning `[::1]:5173` into `[`, and now runs to the closing bracket (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:58`). The comment above it reasoned that the ambiguity did not matter *because* an IPv6 literal is never loopback-by-name; that stopped being true here, so the comment carries the rule instead (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:42-52`) | **Not a refusal** |
| D15a | **`127.0.0.2`** — loopback to the OS, not to this model | Classification | **Existing test** — `only_three_loopback_hosts_are_a_development_callback` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:147`), over every near-miss in this block | Refused, and the message **names the boundary** — "loopback is `127.0.0.1`, `::1` or `localhost`" — rather than reading as a parse failure. `400` / `500` |
| D15b | **`[::ffff:127.0.0.1]`** — the IPv4-mapped spelling | Classification | Same **existing test** (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:147`). **Accepted at `bc1f58c` under `https://`**, because that arm examined no host at all; it is refused now under **both** schemes, which is a deliberate narrowing — a claimed-HTTPS entitlement satisfied by an address that never leaves the machine is the entitlement failing to mean anything | Refused, naming the boundary. `400` / `500` |
| D15c | **`localhost.localdomain`** — resolves to loopback on many machines | Classification | Same **existing test** (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:147`). An entitlement that can only be recognised by resolving a name is not a declaration | Refused, naming the boundary. `400` / `500` |
| D15d | **`app.localhost`, and every other name under `.localhost`** | Classification, as a loopback near-miss (`crates/fabric-client-model/src/identity/redirect_uri/host_kind.rs:136`, `crates/fabric-client-model/src/identity/redirect_uri/host_kind/special_use.rs:64`) | **Existing tests** — `only_three_loopback_hosts_are_a_development_callback` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:147`), which carries `app.localhost` and `a.b.localhost`, and `a_name_under_the_loopback_domain_is_told_what_loopback_is` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:179`), which asserts the message. **RFC 6761 §6.3** requires every name under `.localhost` to resolve to the loopback interface, and Chrome and Firefox do, so this reaches the machine the browser is already on. Accepted at `bc1f58c` as `Https`: `reaches_loopback` tested `localhost.` as a *prefix* only, which catches `localhost.localdomain` and nothing under the domain itself | Refused under **both** schemes, naming the boundary. `400` / `500` |
| D16 | **`post.logout.redirect.uris` is not written** | The wire test | **Proposed test** — `a_declared_client_is_written_with_the_registered_uris_as_its_post_logout_set`, asserting the POST body's `attributes` contains `"post.logout.redirect.uris":"+"` — Keycloak's documented value meaning "the registered redirect URIs". One list, so a second cannot drift out of step with it | Test failure, not a runtime refusal |
| D16a | **Keycloak returns `attributes` on read** | Adapter | **Verified in slice 4 against a real Keycloak.** `ClientRepresentation` reads four fields today and `attributes` is not among them (`crates/fabric-keycloak/src/wire/oidc_client.rs:5-26`), so nothing here has ever observed one. Without the read-back, D16 and C4 are write-only assertions and every sweep would rewrite every client forever | Recorded behaviour, in `docs/verification.md` |

### D17–D19 — the registered-domain rule, the names nobody can register, and the two spellings only the author can resolve

D17's rows are all one change: the `Https` arm stopped asking "is this an IP
address literal?" and started asking "is this a registered domain?". Every
spelling below was **admitted** by the negative rule, and each is refused by a
different part of the positive one, so each has its own message
(`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:71`). The last four rows are a second
family: names that are not addresses at all, and could not be registered
either.

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| D17 | **Any IP literal under `https`** — `93.184.216.34`, `134744072`, `0x08080808`, `[2001:db8::1]`, `[::]` | `registered_domain::check` | **Existing tests** — `an_ip_literal_is_never_a_claimed_https_callback` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:220`), which carries every spelling in this block, and `an_address_literal_is_told_it_is_an_address_and_not_that_it_wants_a_second_label` (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:142`) | `Unadmitted` → `Malformed`, `400`. The message names "a registered domain". A Universal Link and an App Link are claimed against a domain; an address is not one. A host carrying a colon — which reaches here only from a well-formed bracketed IPv6 authority — is told "a registered domain, **not an IP address**" before the label rules run (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:77`); it used to be told it needed at least two labels, which is true of a domain and says nothing about an address |
| D17a | **`https://0x/cb`** — bare, with an empty hexadecimal tail | `registered_domain::check`, at the two-label rule | Same test. A browser reads an empty `0x` tail as 0, so this dials `0.0.0.0` — the machine it is already on. `ip_literal::parse` returns `None` for it, which is why the negative rule admitted it | Refused, `400` |
| D17b | **`https://0x.0x.0x.0x/cb`** | `registered_domain::check`, at the "ends in a number" rule | Same test, plus `a_name_ending_in_a_number_is_an_address_a_browser_would_dial` (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:192`). The final label begins `0x`, which is the URL Standard's test for a host that is an IPv4 candidate rather than a name — read **stricter** here than the standard writes it, which the rustdoc says and argues (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:112`) | Refused, `400` |
| D17c | **`https://１２７．０．０．１/cb`** — fullwidth digits and full stops | `registered_domain::check`, at the ASCII rule | Same test, plus `an_internationalised_host_is_refused_in_favour_of_its_a_label` (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:133`). UTS-46 maps this back to `127.0.0.1` | Refused, `400`, and the message asks for the **A-label** (`xn--`) form — that is the name the browser resolves and the name the claim is made against |
| D17d | **`https://[::1%25lo0]/cb`** — a bracketed zone id | `authority::reject_brackets` (`crates/fabric-client-model/src/identity/redirect_uri/authority.rs:131`) | Same test. A zone id names an interface only one machine has, so `::1%25lo0` is not `::1` and the callback is not a declaration any other machine could act on | Refused, `400`, naming what a bracketed authority is |
| D17e | **`https://[foo]/cb`** — a bracketed name | `authority::reject_brackets` | Same test. A bracketed authority is an IPv6 literal or it is nothing; the general form `[foo.example.com]` is the one that mattered, because it would have reached the registered-domain rule and **passed** it | Refused, `400` |
| D17f | **`https://[::1/cb`** — an unclosed bracket | `authority::reject_brackets` | Same test. **Classified as `Loopback` before this change**: `host_and_port` hands back `::1` for it, on the strength of a bracket whose other half nobody wrote | Refused, `400`. Never `Loopback` |
| D17g | **`https://intranet/cb`** — a single label | `registered_domain::check` | **Existing test** — `a_registered_domain_has_at_least_two_labels` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:256`, and `crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:156`) | Refused, `400`. A single-label name is whatever the resolver in front of the browser decides it is — a search domain, a hosts file, a wildcard resolver — which is not something an entitlement can be stated against |
| D17h | **`https://my_host.example.com/cb`** — an underscore | `registered_domain::check`, at the per-label rule (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain/label.rs:42`) | **Existing test** — `an_underscore_is_not_a_hostname_character` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:265`, `crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs:162`, and `crates/fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain/label.rs:82`), covering the hyphen boundaries beside it | `DisallowedCharacter` → `Malformed`, `400`. Legal in a DNS record, never in a hostname, and no browser will claim an App Link against one |
| D17i | **`https://printer.local/cb`** — the multicast-DNS domain | `special_use::check` (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/special_use.rs:79`), before the registered-domain rule | **Existing tests** — `a_reserved_top_level_domain_is_not_a_claimed_https_callback` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:190`) and `the_multicast_dns_domain_is_refused_and_names_its_rfc` (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/special_use.rs:94`) | `Unadmitted` → `Malformed`, `400`, naming **RFC 6762**. `.local` belongs to multicast DNS, so nobody can register it and no certificate authority can vouch for it — a callback on it can be claimed as neither a Universal Link nor an App Link. Classified as `Https` at `bc1f58c` |
| D17j | **`https://app.test/cb`** | `special_use::check` | Same tests, plus `the_three_rfc_2606_domains_are_refused_and_name_theirs` (`crates/fabric-client-model/src/identity/redirect_uri/host_kind/special_use.rs:102`) | `Unadmitted`, `400`, naming **RFC 2606**, which set `.test` aside permanently |
| D17k | **`https://www.example/cb`** — the reserved TLD, not `example.com` | `special_use::check` | Same tests. `www.example.com` is untouched and is admitted: only the final label is the reservation, which is what a substring test would get wrong (`a_reserved_label_that_is_not_the_top_level_domain_is_an_ordinary_host`, `crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:210`) | `Unadmitted`, `400`, naming **RFC 2606** |
| D17l | **`https://nothing.invalid/cb`** | `special_use::check` | Same tests | `Unadmitted`, `400`, naming **RFC 2606**, which reserves `.invalid` for names guaranteed not to resolve |
| D18 | **`nz.fieldstate.slipway:8080/cb`** — a digit straight after the colon | `private_use_scheme::is_private_use` (`crates/fabric-client-model/src/identity/redirect_uri/private_use_scheme.rs:57`) | **Existing tests** — `a_digit_straight_after_the_colon_is_a_port_and_the_refusal_names_both_readings` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:297`) and `a_digit_after_the_colon_is_a_port_and_the_refusal_names_both_readings` (`crates/fabric-client-model/src/identity/redirect_uri/private_use_scheme.rs:100`) | `Unadmitted` → `Malformed`, `400`. The message names **both** readings, because only the author knows which they meant: a private-use callback's path starts with a slash (`nz.fieldstate.slipway:/cb`), and a host written with its scheme is `https://www.example.com:8080/cb` |
| D18a | **`www.example.com:/cb`** — no digit, so it stays a private-use scheme | Nothing at parse time; the **migrator** refuses it | **Existing test** — `a_v1_callback_that_reads_as_a_private_use_scheme_is_told_both_readings` (`crates/fabric-client-model/src/document/document_tests.rs:235`) | **Not a parse refusal.** A forward domain is as syntactically valid a scheme as a reversed one and there is no digit to say otherwise, so the classifier is right to call it `PrivateUseScheme`. Where an operator meets it is the `v1` migrator, whose message names both readings — migrate under `customScheme`, **or** write the `https://` this host lost |
| D19 | **`https:foo`** — a scheme this model classifies, with no authority | `kind::classify` (`crates/fabric-client-model/src/identity/redirect_uri/kind.rs:112`) | **Existing test** — `a_scheme_with_no_authority_is_told_that_and_not_that_its_scheme_is_wrong` (`crates/fabric-client-model/src/identity/redirect_uri/authority_tests.rs:285`) | `Unadmitted`, `400`, asking for "an authority after the scheme, as in https://host/path". It used to reuse the scheme message, which is false here and points its author at the one part of the URI that is right |

---

## E. Client shape, the schema, and the migration

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| E1 | **A `v2` document still carrying `redirectUris`** | `document/parse.rs`, before typed deserialisation | **Proposed test** — `a_v2_document_carrying_the_replaced_field_is_told_what_replaced_it`, asserting the message names `redirect` and `strategy` rather than serde's `unknown field`. Runs beside `check_document_kind` for the reason already recorded there (`crates/fabric-client-model/src/document/parse.rs:16-20`) | `DesiredStateError::Migration` (new variant), `400 invalid_request` on write; `500 desired_state_invalid` on read |
| E1a | **A `v1` document carrying `redirectUris`** | Nothing — that is the `v1` schema | **Proposed test** — `a_v1_document_keeps_parsing_with_the_field_it_was_written_with`. E1's pre-check applies to `v2` **only** | **Not a refusal** |
| E1b | **A `v2` document parses at all** | Nothing | **Changes an existing test.** `a_future_api_version_is_refused_rather_than_read_as_this_one` (**exists**, `crates/fabric-client-model/src/document/document_tests.rs:60-68`) asserts today that `fabric.fieldstate.nz/v2` is **refused**. It is re-pointed at `v3` in the same change, so the rule it pins — an unknown version is refused, not read as this one — survives with its meaning intact | `v2` parses; `v3` still `UnknownDocumentKind` |
| E2 | **A declared client Keycloak holds as confidential** | Reconciliation, as drift | **Existing test** — `a_declared_client_switched_to_confidential_is_corrected` (`diff_tests.rs:121-130`) | `UpdateOidcClient` |
| E3 | **A confidential client declared in a document** | Not representable | **Structural**, guarded by an **existing test**. `OidcClient` has no secret field and no `publicClient: false` path (`crates/fabric-client-model/src/identity/oidc_client.rs:20-27`); `declaration()` hard-codes `public_client: true` (`crates/fabric-keycloak/src/provider/mutate.rs:123`). `a_declared_client_is_written_as_a_public_authorisation_code_client` (**exists**, `crates/fabric-keycloak/tests/keycloak_adapter.rs:156-178`) asserts the body contains no `secret` (`:174-177`) | Deserialisation failure on any secret-shaped key (`deny_unknown_fields`, `oidc_client.rs:29`) |
| E4 | **An unknown field inside `spec.identity`** | `deny_unknown_fields` | **Existing behaviour** — `IdentityConfiguration` (`crates/fabric-client-model/src/identity.rs:32`) and `OidcClient` (`oidc_client.rs:29`) both carry it | `Malformed`, `400` |
| E5 | **An unknown section elsewhere under `spec`** | Nothing — preserved | **Existing test** — `editing_identity_preserves_every_other_section` (`crates/fabric-client-model/src/document/document_tests.rs:110`). The asymmetry is deliberate (`crates/fabric-client-model/src/document/schema.rs:55-59`) | **Not a refusal** |
| E6 | **A Keycloak vocabulary word leaking above the adapter** | `scripts/check_architecture.py` | **Proposed change** — add `pkce\.code\.challenge\.method`, `post\.logout\.redirect\.uris`, `oidc-audience-mapper` and `included\.custom\.audience` to `check_adapter_containment`'s Keycloak pattern, beside `publicClient` and `standardFlowEnabled` (`scripts/check_architecture.py:826-829`). Without it the containment is decorative for exactly the strings this lane adds | CI failure naming the file and the token |
| E7 | **Two application clients sharing an id** | Model validation | **Existing test** — `two_application_clients_may_not_share_an_id` (`crates/fabric-client-model/src/identity/validation_tests.rs:59`), over `validation.rs:50-69` | `Duplicate`, `400` |
| E8 | **A realm returning more clients than one page holds** | Adapter | **Proposed test** — `a_realm_with_more_clients_than_one_page_is_reported_rather_than_truncated`, mirroring `roles`' existing refusal (`crates/fabric-keycloak/src/provider/observe.rs:12-18`, `:46-50`) and `paths::roles_page`'s bound (`crates/fabric-keycloak/src/admin/paths.rs:53-59`), which `paths::clients` (`:61-64`) lacks | `ProviderError::Rejected`, naming the cap. Never a partial reconciliation |

### E9–E16 — the `v1` migrator, and the migration an edit performs

Each `v1` client's `redirectUris` is classified by `RedirectUriKind`, and the
whole list must agree. Tests live in `crates/fabric-client-model/src/document/`.

| # | Case | Refused by | Provability | Expected |
|---|---|---|---|---|
| E9 | **All entries `Https`** | Nothing | **Proposed test** — `a_v1_client_with_only_public_callbacks_reads_as_claimed_https` | `RedirectStrategy::ClaimedHttps { uris }`. Not a refusal |
| E10 | **All entries `.internal`** | Nothing | **Proposed test** — `a_v1_client_with_only_internal_callbacks_reads_as_private_network` | `RedirectStrategy::PrivateNetwork { uris }` |
| E11 | **All entries loopback** | Nothing | **Proposed test** — `a_v1_client_with_only_loopback_callbacks_reads_as_development` | `RedirectStrategy::Development { uris }` |
| E12 | **A mix** — `https://www.example.com/cb` **and** `http://localhost:5173/cb` | The migrator | **Proposed test** — `a_v1_client_mixing_callback_kinds_must_be_migrated_by_hand`, asserting the message names `v2` and `spec.identity.clients[].redirect`. Refused rather than resolved: picking the looser strategy would silently grant an entitlement the operator never stated | `DesiredStateError::Migration`, `400` on write; **`500 desired_state_invalid`** on read of a stored one |
| E13 | **A private-use scheme in a `v1` list** | The migrator | **Proposed test** — `a_v1_client_with_a_private_use_scheme_must_be_migrated_by_hand`. Cannot arise from a document `v1` could hold — at `bc1f58c` `RedirectUri` refused every scheme but `http` and `https`, in the single `authority::check` the rule lived in then — so the test constructs it directly. The arm exists so the migrator stays **total** once `RedirectUri` widens in this same slice | `DesiredStateError::Migration`, `400` / `500` as E12 |
| E14 | **An edit to a `v1` document migrates it to `v2`, in place** | Nothing — this is the migration | **Proposed tests** — `editing_a_v1_document_returns_a_v2_document` and `the_migrated_version_keeps_its_position_in_the_file`, in `crates/fabric-client-model/src/document/document_tests.rs`, over `with_identity` (`crates/fabric-client-model/src/document/render.rs:28-52`). **Mechanically forced, not a preference**: `with_identity` re-parses the rendered text (`render.rs:50-51`) so "there is no path that produces a document this model would later refuse" (`render.rs:22-27`), and a `v2` `identity` block under a `v1` `apiVersion` fails that re-parse. Key order is preserved because an ordered mapping's `insert` replaces in place (`render.rs:45-48`), which the existing `an_edit_preserves_the_order_keys_were_written_in` (`document_tests.rs:198`) already pins | **Not a refusal.** `apiVersion: fabric.fieldstate.nz/v2`, the `v2` client shape, every other section and its order untouched. A `v1` file nobody edits stays `v1` — nothing reinterprets a document at rest |
| E15 | **`v1` clients still get S256 on the next sweep** | Nothing — the deliberate runtime break | **Proposed test** — `a_v1_client_is_still_reconciled_with_the_s256_challenge_method`. **This is the break ADR 0019 names**, and the test is what makes it visible rather than surprising | **Not a refusal.** `UpdateOidcClient` on the first sweep after deployment. Every public client not already performing PKCE stops working |
| E16 | **The shipped examples still parse, and cover every path** | Nothing | **Extends an existing test** — `every_example_client_document_parses` (**exists**, `crates/fabric-control-plane-api/tests/example_configuration.rs:92-110`) walks the whole `examples/clients` directory, so a third document is covered the moment it lands. `acme.yaml` migrated to `v2` (`examples/clients/acme.yaml:31-35`); **`initech.yaml` added, on `v1`, with one client**, because without it the only `v1` example declares `clients: []` (`examples/clients/northwind.yaml:26`) and **the migrator is never reached by a shipped document at all**; `northwind.yaml` untouched | All three parse. One exercises `v2`, one exercises the `v1` migrator, one is the cheapest proof `v1` still parses |

---

## F. The composed proof

`docs/delivery.md:5-7`'s rule: one test through the surface an operator actually
uses, doing the thing the slice exists to let them do.

**Proposed** — `crates/fabric-control-plane/tests/control_plane_api.rs`,
`a_native_client_is_declared_reconciled_and_read_back_with_its_pkce_and_strategy`:

```text
GET identity → read the revision
PUT identity declaring a v2 native client: pkce s256, strategy development,
    loopback callback, with If-Match
→ 200, reconciliation pending
sweep against the fake provider
→ the provider holds one public client with the S256 challenge method, the
  audience mapper, the post-logout attribute, and exactly the declared callback
GET identity → the strategy and method come back as written
PUT the same client with strategy claimedHttps and the loopback callback
→ 400 invalid_request, naming the strategy and the URI
```

**Proposed** — `crates/fabric-keycloak/tests/keycloak_adapter.rs`,
`a_declared_native_client_round_trips_through_the_wire_unchanged`: write the
declaration through `create_oidc_client`, then feed the recorded POST body back
as the fake's `GET /clients` response and call `observe_realm`, asserting the
plan is converged.

**The `id` splice.** `NewClientRepresentation` carries no `id`
(`crates/fabric-keycloak/src/wire/oidc_client.rs:34-63`) while
`ClientRepresentation` requires one (`:5-13`) — Keycloak generates it. The
existing fake supplies `"id":"uuid-1"` by hand
(`crates/fabric-keycloak/tests/keycloak_adapter.rs:47-51`). So the round trip
splices an `id` into the recorded body before serving it as the read, and
nothing else — any other edit and the test stops proving that the write and the
read agree.

This is what proves the write and the read agree about PKCE, about the audience
mapper, and about redirect URIs. A write-only assertion would pass while
`observe` dropped the attribute and every sweep rewrote the client forever.

**What the round trip still cannot prove** is that Keycloak sends what the fake
was handed. That is D14c, D16a and A13's read-back half, and it is why those
rows say *verified in slice 4 against a real Keycloak* rather than *proposed
test*.

**Proposed** — `crates/fabric-data-api/tests/identity_boundary.rs`,
`a_token_cannot_name_a_tenant_its_issuer_does_not_own`: the §2 binding through
the real router, asserting `401` and `query_count() == 0`. The second half is
the load-bearing one — a refusal that still hit the connector would mean the
tenant predicate had already been chosen.

---

## G. What this matrix cannot prove, and who proves it

Every row is an obligation on `saas-fabric-platform` unless stated otherwise,
and maps to the §G row of the same number in
[ADR 0019](../decisions/0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md).

| # | Obligation | Owner | Evidence expected |
|---|---|---|---|
| G1 | One gateway route per runtime service terminates `/v1/data/*` with a JWT policy attached, and is the only route in. One route, **many tenants** | `saas-fabric-platform` | The resource in Git; `check.py` fails if the route has no policy |
| G2 | Per-issuer JWKS URI, reachable from the gateway's network position | `saas-fabric-platform` | The resource in Git |
| G3 | Algorithm allow-list **per issuer**; `none` and every HMAC refused | `saas-fabric-platform` | The resource in Git. Not a global constant, and not `fabric-identity`'s private list |
| G4 | Issuer allow-list = the **set** of registered issuers, exact match, non-empty, no duplicates. The edge decides membership; the runtime decides the tenant | `saas-fabric-platform` | The resource in Git; the runtime refuses to start without its own registry (B11, B12) |
| G4a | Both artefacts — the gateway's allow-list and the runtime's `[identity].trusted_issuers` — are **generated from one tenant list, in one change** | `saas-fabric-platform` | The generator in Git, and a check that the two outputs agree. This is the only control over B15, which nothing else catches |
| G5 | One audience string per deployment, required on every token, **equal to every `IssuerRegistration.audience`** | `saas-fabric-platform` | The resource in Git; A13a asserts the equality inside this repository |
| G5a | The `aud` check is enabled only **after** the first successful sweep writes the mapper | `saas-fabric-platform` runbook | The runbook; A13 proves the mapper is written |
| G6 | `exp` required at the edge — the runtime will not refuse an `exp`-less token | `saas-fabric-platform` | The resource in Git; A14 pins the runtime's side |
| G7 | Clock skew ≤ 30 s at the gateway | `saas-fabric-platform` | The resource in Git; A9's unit test pins the two in-repo constants the bound is derived from |
| G8 | Three failure classes, with `503` for JWKS-unreachable and for a cooldown-suppressed refresh; unknown `kid` is `401` only against a fresh successful snapshot | `saas-fabric-platform` | The resource in Git; a negative test with the JWKS endpoint blackholed, asserting `503` and **not** `401` |
| G9 | `X-Tenant-Id`, the `x-jwt-claim-*` prefix, `x-forwarded-user`, `x-auth-request-*` and the operator header are stripped from inbound requests | `saas-fabric-platform` | Header-modifier filter in Git; a test asserting a spoofed `x-jwt-claim-tenant-id` does not reach the runtime |
| G9a | The gateway projects **no** verified claim into any header — no `claim_to_headers`, no emission under a name of its own | `saas-fabric-platform` | The absence in Git, reviewed as a policy rather than assumed; a test asserting the forwarded request carries `Authorization` and no `x-*` claim header (A15) |
| G10 | `Authorization` forwarded byte-for-byte | `saas-fabric-platform` | The resource in Git |
| G11 | Runtime services unreachable except from the gateway | `saas-fabric-platform` | `NetworkPolicy` in Git; a negative test from a scratch pod in the cluster (A10) |
| G12 | Edge `401` shape: `WWW-Authenticate`, no diagnostic body, never a `302` | `saas-fabric-platform` | The resource in Git |
| G13 | Edge `503` shape: `Retry-After`, no credential description | `saas-fabric-platform` | The resource in Git |
| G14 | Two real Keycloak realm users complete authorization-code + S256 PKCE through a deployed runtime behind this edge | M2 acceptance run | A recorded run, both users, plus C7's refusal observed |
| G15 | An intercepted code cannot be redeemed without the verifier | M2 acceptance run | C8 |
| G16 | Keycloak's real behaviour: any-port loopback (D14c); whether `GET /clients` returns `protocolMappers` and `attributes` (A13, D16a); whether `PUT /clients/{id}` updates mappers or `/clients/{id}/protocol-mappers/models` is required | The adapter slice | Recorded in `docs/verification.md` beside the 2026-08-28 findings (`docs/verification.md:487-524`); amends ADR 0019 §3 or §6 **with the evidence** if it contradicts them |
| G17 | The same probe re-run on **Keycloak 26.7.2**, the version LucentRoot runs. G16's findings are from **26.0.8**, the image `scripts/e2e-services.sh` uses, observed 2026-09-06 | `saas-fabric-platform` | The probe output recorded in `docs/verification.md` beside the 26.0.8 findings. D14b and the `Development` row were amended from 26.0.8's evidence; a difference on 26.7.2 amends them again rather than being absorbed |
