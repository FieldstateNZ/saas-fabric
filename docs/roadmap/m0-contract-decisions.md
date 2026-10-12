# M0 — Fabric lifecycle and authorization contract decisions

Deliverable for [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108)
(W0, gate M0, planning key `D01`). This is read-only analysis. It changes no
code, applies nothing to a provider, changes no credential and merges nothing.

| Source | Ref | Head read |
|---|---|---|
| `FieldstateNZ/saas-fabric` | `main` | `1f46788ecd5c59af1a4937aae0e51cf59b8f8081` |
| `FieldstateNZ/saas-fabric` PR #93 | `claude/adr-0025-gate-and-accounts` | `647ef8dd808fc59184e467991c5dee681a90d16b` (base `8676af4`) |
| `FieldstateNZ/saas-fabric-platform` | `main` | `0ff5d669d36a6c5c0b133bf4e6ced74764504f11` |
| `FieldstateNZ/saas-fabric-platform` PR #45 | `pull/45/head` | `fd12327373820b545ddcf33bd84887a4c3c18a50` (parent `355fe6d`) |
| Date | | 2026-10-05 |

**Reading rules.**

- "Decided" means an ADR with status Accepted. Nothing else counts as decided.
- A Proposed ADR is reported as *Proposed, built* or *Proposed, not built*,
  even where code implements it.
- Architecture documents and code that no Accepted ADR covers appear under
  "Documented or built, not an Accepted ADR".
- Cluster facts are not re-observed here. Anything this document could not
  check is marked **not verified**.
- Brett (product owner) owns every unresolved policy choice. D01-26 is an
  engineering ownership choice under Accepted ADR 0024, and says so.
- Recommendations are labelled as such. They are not decisions.

ADR status at `1f46788`:
- **Accepted:** 0001–0003, 0005–0017 (0014 for the control plane only), 0020,
  0022 (LucentRoot readiness milestone only), 0024, 0025.
- **Superseded:** 0004.
- **Proposed:** 0018, 0019, 0021, 0023, 0026.

---

## 1. Provisioner and publication ownership

### Decided

- **Desired state is the authority:** ADR 0008 (Accepted).
- **Isolation is checked against an observed fact, not a label:** ADR 0007
  (Accepted). It supersedes ADR 0006's rule and keeps that rule as one of its
  two conditions: a shared data source serves discriminator isolation only
  (ADR 0006, Accepted, extended by 0007).

### Documented or built, not an Accepted ADR

- **The platform repository owns wiring, not data, for the runtime documents.**
  The platform declares the Role, the RoleBinding and the whole-volume mounts.
  The three `fabric-runtime-*` ConfigMaps are declared nowhere in Git.
  - This is stated in ADR 0018 and ADR 0023, both **Proposed**.
  - It is built on both sides: platform #40 (per the #70 comment of
    2026-09-20) and platform
    `applications/core/saas-fabric/base/configmap.yaml:25-39`.

### Built under Proposed ADRs (evidence, not decision)

| Contract | ADR (status) | Code | Tests |
|---|---|---|---|
| Three versioned documents, monotonic revision, byte-compared divergence, refuse emptying and dangling references | 0018 (Proposed) | `crates/fabric-runtime-publication/src/validate.rs:59-136`, `src/verdict.rs` | `verdict_tests.rs::an_older_revision_against_a_held_manifest_and_payload_is_refused` (:66), `::the_same_revision_with_different_bytes_is_refused_as_divergent` (:81); `tests/published_state_serves_two_tenants.rs::a_stale_revision_publication_is_refused_and_the_last_good_files_remain` (:262), `::an_emptying_publication_is_refused_unless_it_is_intended` (:463); mutation table `docs/verification.md:228-305` |
| The publisher is a control-plane controller with its own ServiceAccount, running on a schedule and on `POST /api/platform/publication` | 0023 §4 (Proposed) | `fabric-platform-management/src/publication/*`; the `/api/platform/publication` line of the route list in `crates/fabric-control-plane/src/routes.rs` | `crates/fabric-control-plane/tests/platform_publication.rs::publishing_composes_declared_state_into_the_runtimes_three_documents_and_settles_unchanged` (:200), `::a_catalogue_with_no_published_resources_is_waiting_and_writes_nothing` (:353) |
| Kubernetes adapter over plain HTTPS: `get/create/update`, no `delete`, no `kube` crate | 0023 §4, amending 0018 (Proposed) | `crates/fabric-publication-kubernetes/src/publish.rs` | `publish_tests.rs::a_first_publication_creates_three_objects_in_order_with_the_label` (:24), `::a_stale_snapshot_is_refused_before_any_write` (:178), `::a_document_past_the_object_cap_is_refused_before_any_write` (:207) |
| Data sources are environment desired state (`data-sources.yaml`); placement is a recorded Fabric write (`placements.yaml`) | 0023 §1–2 (Proposed) | `crates/fabric-platform-management/src/placements/select.rs:26-56` | `placements/select_tests.rs::rule_3_a_shared_candidate_isolates_by_the_tenant_id_as_the_discriminator_value` (:257); `crates/fabric-control-plane/tests/platform_placements.rs::a_clients_data_intent_is_placed_recorded_and_protects_its_data_source` (:180) |

**Deployed evidence.** The #70 comment of 2026-09-22 records platform #41
merged as `dc5323a`. That change enabled `[platform_management.publication]`
on LucentRoot, with an expected status row of `waiting`. **Not verified:**
whether the row now reads `waiting`, `published`, `refused` or `failed`.

### Unresolved

**U1.1: who provisions a tenant's database, schema and connector process.**
This is part (a) of **D01-3**.

The evidence:
- ADR 0018 names `ProvisionedPlacement` as an input with "no owner yet"
  (`docs/decisions/0018-…:512-579`).
- ADR 0023 refuses both provisioning and the connector deployment
  (`docs/decisions/0023-…:389-405`).
- Brett's direction (#70, 2026-09-22): *a connector definition is a platform
  contract; provisioning is per client; ADR to follow*. No such ADR exists at
  `1f46788`.
- ADR 0026 leaves "connector kinds" undecided
  (`docs/decisions/0026-…:890-907`).

D01-3 covers three per-client questions, because they share one mechanism:
(a) this one, (b) U2.5 and (c) U4.4.

| Option | Shape | For | Against |
|---|---|---|---|
| A | **Platform composition per client.** An OpenTofu Application per client, following the `master-instance` pattern (ADR 0025), creates the database and role, the connector Deployment and its `[[connectors]]` entry. Fabric declares the data source and records the placement | The mechanism is already proven for the master realm; it keeps ADR 0012's "no standing credential in the product"; Git explains the environment | Every new client needs a platform change; Fabric's client creation needs a way to trigger the platform repository |
| B | **A Fabric controller provisions**, using its own database-admin credential | One write path; client creation can complete end to end | Reverses ADR 0012's posture for a new provider; puts a standing high-privilege credential in the control plane |
| C | **Pre-provisioned pools only** (status quo). Operators declare shared data sources; dedicated databases are made by hand | Nothing new to build | Dedicated tenants can never self-serve; the connector deployment still has no owner |

*Recommendation:* **A**. It matches Brett's 2026-09-22 direction and the
boundary ADR 0025 draws. Decision owner: **Brett**.

**U1.2: whether ADR 0018 and ADR 0023 are accepted as built** (**D01-1**,
**D01-2**). Both are implemented, wired on LucentRoot and still Proposed. ADR
0026 says that accepting it accepts the parts of 0021 and 0023 it builds on
(`docs/decisions/0026-…:18-20`).

ADR 0023 contradicts itself on non-shared placement:
- Decision part 2 (`:222-228`) and Consequences (`:370-372`) say dedicated
  placement is refused until provisioning exists.
- `:230-235` says any non-shared class holds at most one tenant, and the first
  tenant to ask takes it.
- `select.rs:44-47` implements `:230-235`.

The amendment that fixes this belongs to the heading at `:222` and to the
Consequence. It is part of D01-2.

Other text owed:
- `README.md:451-454` still says "the console does not yet show that row".
  #81 built that row.
- Accepting ADR 0023 as built also accepts §5's manual raise
  (`:316-325`). Choosing option B for D01-18 therefore means amending §5.
- D01-1 includes runtime-document rollback (U5.3), because ADR 0018 §6 already
  entails it.

Options:
- **A.** Accept both with their "as built" amendments.
- **B.** Amend first (the points above), then accept.
- **C.** Leave both Proposed.

*Recommendation:* **B**. Owner: **Brett**.

**Disposition (2026-10-11, [#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): B for both.**
- **D01-1:** amend ADR 0018 to the verified contract, then accept it through
  the reviewed change. Runtime documents roll forward only, so earlier
  content becomes a new revision and never resets one. Tenant and resource
  revisions stay monotonic, as well as document revisions.
- **D01-2:** amend ADR 0023 before acceptance. Taking a compatible dedicated
  source that is already declared is distinct from provisioning a new one,
  and no automatic provisioning is claimed. Platform wiring and control-plane
  data ownership are preserved, the stale console documentation is fixed,
  and activation stays explicit and readiness-gated.
- **Remaining gaps:** #112's reader-visible and atomicity gaps are
  reconciled in bounded PRs, with compatibility and partial-publication
  failure tests.
- **Where it is recorded:** ADR 0018 part 10 and its "Owed" list, and ADR
  0023's amendment and its "Owed" list. Acceptance does not verify the owed
  items O1–O8 (see [Dispositions recorded](#dispositions-recorded)).

---

## 2. Identity route and session ownership

### Decided

- **Trusted ingress is the canonical runtime posture:** ADR 0002 (Accepted).
  The platform's runtime config sets `mode = "trusted_ingress"`
  (`saas-fabric-platform: applications/core/saas-fabric/base/configmap.yaml:23-24`).
- **Operator identity is not tenant identity:** ADR 0009 (Accepted).
  Operators sign in against the platform (master) realm: ADR 0010 (Accepted).
- **Sign-in belongs to the gateway, and the browser never holds a token:**
  ADR 0024 (Accepted 2026-09-22). The same ADR fixes one identity route,
  `GET /api/user/current`, and makes product surfaces federated modules.
- **The master realm's instance resources are platform composition:** ADR 0025
  (Accepted 2026-09-22). These are the gateway client, the `fabric-operator`
  role and the operator grants. ADR 0012 still governs client realms.

What is built:

- **ADR 0024 slice 1, Fabric side.** `GET /api/user/current` is the probe for
  a gateway session (it was `GET /api/operator` until slice 2). Both answer
  `401 operator_refused` when the `Operator` extractor refuses a forwarded
  bearer (`crates/fabric-control-plane/src/operator/extractor.rs`).
- **ADR 0024 slice 2: built in source (D01-9 and `/api/user/current`, 2026-10-12).**
  - `GET/POST /api/session`, the `SignInSurface` port, the Keycloak adapter's
    code redemption and the operator posture's `redirect_uri` are removed.
    `tests/boundary.rs::the_control_plane_redeems_no_authorization_code_for_anybody`
    pins the route as gone;
    `example_configuration.rs::a_configuration_still_stating_the_retired_redirect_uri_refuses_to_load`
    pins the configuration break.
  - The console is session-free: no PKCE, no token, no `Authorization`
    header; it probes `/api/user/current` and navigates to the gateway's
    sign-in.
  - **Platform action owed:** LucentRoot still states `redirect_uri`
    (`applications/core/saas-fabric-control-plane/overlays/lucentroot/control-plane.toml`,
    whose comment already says it "retires alongside that endpoint in ADR
    0024 slice 2"). It must be dropped in the change that takes this release,
    which is also when the console depends on gateway sign-in alone.
  - `GET /api/user/current` answers ADR 0024 §3's shape behind the unchanged
    `Operator` extractor, by the disposition in
    [#108 comment 6119067476](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6119067476):
    the exact `sub`, `preferred_username`/`email` or `null`, a display name
    that is never empty, and the configured `required_role` alone as `roles`
    (`crates/fabric-control-plane/src/handlers/get_current_user.rs`). Signed
    tokens through the real authenticator are tested in
    `handlers/get_current_user/get_current_user_tests.rs`, and the full
    router in `tests/user_current.rs`. `/api/operator` keeps answering,
    subject only, for one release, and the console no longer calls it: its
    Settings page reads `/api/user/current` too.
- **ADR 0024 slices 3–5: not built.** `apps/app-shell` has no sign-in and does
  not load modules at runtime. The runtime has no `/v1/identity` route.
- **Platform side.** There is an OIDC `SecurityPolicy` at
  `applications/core/saas-fabric-control-plane/overlays/lucentroot/oidc.yaml`,
  and the `master-instance` convergence exists (platform #43, `355fe6d`).
  **Not verified:** the first sign-in through the gateway.
- **Operator roster: the evidence conflicts, and the cause of `Degraded` is
  not established.**
  - Platform `main` (`0ff5d66`) has
    `master-instance-config.yaml:40` = `operators: '["brett@fieldstate.nz"]'`.
    The lookup is by Keycloak *username*, not email (`:33-35`), so the value
    names a username.
  - Supporting "the account exists":
    - platform `applications/core/master-instance/README.md:110-113` on
      `main` says the grants were made by hand and "are now converged";
    - the #70 comment of 2026-09-22 08:22Z says the same.
  - Supporting "the account does not exist":
    - PR45's README and commit message, which is unmerged, say the master
      realm holds only `admin` and that three runs failed at plan time on
      2026-09-22;
    - PR93's body says the same.
  - #108's Oct-3 inventory reports "degraded master-instance convergence" and
    gives no cause.
  - Diagnosis belongs to platform #48, "[W2] Diagnose and prove state-aware
    master-instance convergence".
  - **Not verified:** whether a master-realm user named `brett@fieldstate.nz`
    exists today, and whether the roster caused the Oct-3 `Degraded`.

### Runtime tenant identity (ADR 0019, Accepted as amended 2026-10-12, D01-5)

**Built: the runtime half.** ADR 0019 §2 is in code:
- the issuer names the tenant;
- the token's tenant claim must agree;
- an empty registry refuses to start.

Code:
- `crates/fabric-identity/src/resolver/tenant_binding.rs:45-57`
- `src/identity/trusted_issuer.rs:108-131` (`validate_registry`)

Tests:
- `crates/fabric-identity/src/resolver_tests.rs::a_token_from_an_unregistered_issuer_is_refused` (:126)
- `::a_token_with_no_issuer_is_refused_rather_than_treated_as_unregistered` (:139)
- `::a_tenant_claim_that_disagrees_with_its_issuer_is_refused` (:165)
- `crates/fabric-identity/src/config.rs::a_runtime_with_no_trusted_issuers_refuses_to_start` (:197)

The runtime's half of §1 under D01-5's single-route model is tested in
`crates/fabric-api/tests/edge_trust_contract.rs`:
- `the_runtime_serves_no_path_into_tenant_data_but_the_data_api`: with an
  admitted bearer, every shell-, module-, session- or tenant-prefixed path a
  client route could reach answers `404`, and no connector is reached;
- `every_data_api_route_refuses_a_request_that_carries_no_bearer`: every
  method on every `/v1/data` path shape answers `401` before a connector;
- `the_edge_leeway_ceiling_is_the_smallest_downstream_allowance`: §G7's 30 s.

**Accepted, not built: the edge half.** This is the §G checklist, G1–G17
(`docs/decisions/0019-…:1320-1339`). D01-5 gives it to the platform.
- The platform runtime `HTTPRoute`
  (`applications/core/saas-fabric/base/httproute.yaml`) has no JWT policy and
  uses the host `fabric.invalid`.
- Its header comment (`:3-4`) says client hostnames such as `acme.<domain>`
  get routes "created by OpenTofu in the client's own namespace". It does not
  say whether those routes reach `/v1/data`. That conflicts with G1 ("one
  route per runtime service … no per-tenant route") only if they do.
  **D01-5 settles the intent: they do not** (ADR 0019, amendment
  2026-10-12). **Platform action owed:** say so in that comment. What the
  routes actually do is not verified.
- The runtime config has no `[identity]` section, so the runtime would refuse
  to start even with its documents published (`trusted_issuer.rs:111`).
- The runtime is held at `replicas: 0`
  (`applications/core/saas-fabric/base/deployment.yaml:20`).

### Unresolved

**U2.1: accept ADR 0019, and decide who owns the edge (§G)** (**D01-5**).

Options:
- **A.** Accept as written. The platform implements G1–G17, and its route
  comment is clarified to say client routes do not reach `/v1/data`.
- **B.** Amend G1 to allow per-client routes to the runtime's `/v1/data`,
  each carrying the same JWT policy.
- **C.** Defer the edge and keep the runtime at zero replicas.

*Recommendation:* **A**. The runtime half is built and tested against the
single-route model. Owner: **Brett**.

*Disposed 2026-10-11, built 2026-10-12:* **A**. ADR 0019 is accepted as
amended. The amendment states that client UI routes add no path into
`/v1/data`, guarded or not, and the runtime's half is tested (above). Owed
to ADR 0024 slice 5, as a gap and not a built journey: how a browser surface
on a client instance reaches the Data API, given that ADR 0024 keeps tokens
out of the browser, and whether a runtime `/v1/identity` joins G1
([#108 disposition](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6116986746)). Live JWT and trust configuration stays behind its
separate security gate. See [Dispositions recorded](#dispositions-recorded).

**U2.2: the generator for the issuer-to-tenant registry (ADR 0019 §G4a)**
(**D01-4**). Nothing generates it: see #70 and `README.md:455-457`. ADR 0023
rejected "a fourth document, now" (`docs/decisions/0023-…:387`).

| Option | Shape | For | Against |
|---|---|---|---|
| A | **The Fabric publisher generates both lists** from client realms and placements: the runtime registry inside the published tenants document (an ADR 0018 `v2` change), and the gateway's issuer allow-list | One generator, one change, as G4a requires; the realm list is already Fabric's | Schema change to a `deny_unknown_fields` document. Reverses ADR 0023's Consequence that the registry "is still configuration" (`0023-…:373-376`). A Fabric-written gateway object widens the publisher's RBAC. If the platform renders the allow-list instead, from a list Fabric publishes, that risks breaking G4a's "in the same change" (`0019-…:1324`) |
| B | **Platform composition** generates both per client (the D01-3 Application) | Matches ADR 0025's boundary: the edge belongs to the platform | Two repositories must agree on the tenant list, while Fabric's placement is the source of truth for which tenants exist |
| C | Hand-maintained `[identity].trusted_issuers` and gateway list | Fastest route to one trial tenant | Exactly the drift G4a forbids |

*Recommendation:* **A** for the runtime registry. The sub-choice is whether the
gateway allow-list is written by Fabric (wider RBAC) or rendered by the
platform (risk to G4a). Owner: **Brett**.

**U2.3: how an operator's master-realm account comes to exist** (**D01-6**).
The convergence grants roles to accounts and creates none (PR93, PR45).

Options:
- **A.** Broker to an upstream identity provider. This is Karo's model; Brett
  names the upstream.
- **B.** The convergence creates declared local accounts.
- **C.** The bootstrap `admin` stays the only operator.
- **D.** An empty roster `[]`. The README (`:117-118`) says this is valid,
  grants nothing and blocks no sync.

*Recommendation:* C as the interim and A as the target. Making the roster
true is a PR disposition (see "PR dispositions") and needs authorisation to
deploy. Owner: **Brett**.

Sub-choice **D01-6b**, operator revocation (formerly D01-7):
- Grants are `exhaustive = false`, so removing a name revokes nothing
  (README "Who the operators are").
- Options:
  - **A.** Manage grants exhaustively in the convergence.
  - **B.** Revocation stays an act in Keycloak, and is documented.
  - **C.** Revoke at the upstream IdP once D01-6 is A.
- *Recommendation:* **C** if D01-6 is A, otherwise **A**.

**U2.5: who provisions a client *instance*** (its realm's gateway client, host
and OIDC policy). This is part (b) of **D01-3**, formerly D01-8.
- ADR 0025 leaves this open and names Karo's "every directory alike" as the
  expected answer (`docs/decisions/0025-…:133-140`).
- Client realms themselves stay with the control plane under ADR 0012.

Options:
- **A.** Platform composition per instance; Fabric keeps reconciling realm
  content.
- **B.** The control plane creates the instance resources, acting as the
  operator (ADR 0012's posture).
- **C.** Every client reuses the master pattern unchanged.

*Recommendation:* **A**. Owner: **Brett**.

**U2.6: the loopback workbench, and retiring `/api/session`** (**D01-9**).
**Disposed 2026-10-11 and built 2026-10-12.** ADR 0024 §2 retires
`/api/session`; ADR 0021's owed decision 1 asked whether to keep the loopback
workbench.

Disposition ([#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)):
retire the workbench and `/api/session` only after moving the disposable
test and runbook to the accepted gateway flow or explicitly parking that
profile; keep local automated tests possible; do not silently invalidate
#101/#111.

What was done:
- **The disposable dogfood profile (#101, #111) is parked, not moved.** No
  gateway flow exists that could be built and tested offline: ADR 0024 slice
  1's local half is not built, and adding a gateway to the trial would add a
  confidential client, its secret and a cookie key, which this change was not
  authorized to introduce. The profile stays pinned to `bb864f4`, which still
  serves `/api/session`; its runbook and every activation packet say it is
  parked, unsupported and not part of any deploy
  (`examples/disposable-dogfood/README.md`, "Parked (D01-9)").
- **Its offline tests still run, now on every pull request.** CI's `scripts`
  job runs `scripts/tests`, which previously ran only on the profile's own
  branch workflow. `scripts/tests/test_dogfood_parking.py` proves the parking:
  the pinned commit serves the route, the working tree does not, and no
  release, image or hosting workflow refers to the profile.
- **Then the workbench and `/api/session` were removed**, with the console's
  PKCE sign-in, its `preview:ui` entry, and `redirect_uri`; see "Identity
  route and session ownership" above for what is built and the platform
  action owed. ADR 0021 (amendment 2026-10-12) and ADR 0024 ("Implementation
  status") record it.

Reviving the profile means moving its sign-in stage and readiness checks to
the gateway flow once the local gateway exists, in one reviewed change.

**U2.7: who configures tenant end-user sessions for client instances**
(**D01-26**). This is an engineering ownership choice under an existing policy,
not a policy vote.

The policy is decided. ADR 0024 (Accepted, 2026-09-22) says the gateway holds
the session: it resolves the realm from the host, runs the authorization-code
flow, keeps the session in gateway cookies and forwards the access token as a
bearer, and the browser never holds a token (§1 `docs/decisions/0024-…:78-86`,
§2 `:88-97`). `/v1/identity` answers on the runtime (slice 5, `:162-163`). What
no ADR fixes is who writes each client instance's gateway session
configuration (host, OIDC provider and realm, cookie secret) and when. This is
the per-client gateway half of U2.5 (D01-3(b)). #114 (M3, open) proves the
edge and session boundaries once they are configured; it does not assign who
configures them.

Evidence:
- ADR 0024 Context describes the adopted Karo shape: one Envoy filter chain
  per client, selected by host (`:44-56`). Its decision names a
  `SecurityPolicy` on LucentRoot (`:80-81`).
- ADR 0025 puts the edge with the platform and sets the gateway secret
  in-cluster, with rotation by convergence re-run (`0025-…:66-72`). It names the
  same approach for client instances as the expected answer (`:135-138`).
- ADR 0012 keeps client realm content with the control plane.
- The same cost appears in U2.2 A: a Fabric-written gateway object widens the
  publisher's RBAC.

Options:
- **A.** Platform per-client composition. The D01-3 Application that creates
  the instance's gateway client also renders its `SecurityPolicy`, route and
  cookie secret. For: one mechanism with U2.5 A, and the edge stays with the
  platform. Against: every new client needs a platform change.
- **B.** The Fabric control plane renders and applies the gateway session
  configuration, acting as the operator. For: client creation completes end
  to end. Against: a standing credential and wider RBAC for gateway objects in
  the control plane, and it holds the cookie-signing secret.
- **C.** One platform-owned policy template, with Fabric publishing only the
  realm and host list (the D01-4 generator). For: one gateway object to
  review. Against: it needs the generator first. **Not verified:** whether a
  single Envoy Gateway `SecurityPolicy` can select a realm per host, when ADR
  0024 Context describes one filter chain per client (`:44-56`).

*Recommendation:* **A**, decided together with D01-3(b). Owner: engineering
ownership under ADR 0024; **Brett** confirms only if the answer changes
D01-3(b). A is engineering ownership within ADR 0024 and ADR 0025
(`0025-…:135-138`). Choosing B or C changes ADR 0025's boundary and would need
an ADR and Brett. **Not verified:** whether ADR 0024 slices 3–5 are built.

---

## 3. Authorization enforcement

### Decided

- **Authorization is declared in the platform's words:** ADR 0013 (Accepted).
  A client declares resources, the relations on them and the operations each
  relation permits. Memberships are runtime data, not desired state. Model:
  `crates/fabric-client-model/src/authorization.rs`.
- **Fabric owns the OpenFGA front door:** ADR 0016 (Accepted). Identity is
  bound from the verified `iss`, and there are two listeners (a runtime
  surface and a control-plane surface). The runtime surface is built in
  `crates/fabric-fga-auth/`; its registry pins `authorization_model_id`
  (`src/registry.rs:129-131`).
- **The control plane calls OpenFGA as the operator:** ADR 0014 (Accepted, for
  the control plane only).
- **Fabric decides which client secret boundary an operation reaches:**
  ADR 0017 (Accepted).
- **Operator authority is coarse and separate from tenant identity:** ADR 0009
  (Accepted). `docs/verification.md`, "What is not verified", says: "Every
  authenticated operator may do everything the API offers."

### What actually enforces today

- **The Data API enforces layered authorization, with only layer 1 active**
  (ADR 0027, Proposed):
  - Layer 1 is the scope `data:<resource>:read` or `data:<resource>:write`.
    It runs on every request. There is no administrator role and no
    `require_scopes` toggle, and a `[permissions]` section is refused at load
    (D01-12).
  - Layer 2 is the authorization front's Check, as the caller. It runs for a
    tenant whose `TenantPosture` is not `SyntheticTrial`, on a resource that
    declares relations, and it fails closed. It is built and tested
    (`crates/fabric-data-api/src/authorization/`, `crates/fabric-fga-check/`)
    and not wired: `fabric-api` composes `DataAuthorization::default()`, which
    has no tenant policies and no configured Check.
  - Tests that information about withheld verbs does not leak:
    `crates/fabric-data-api/tests/authorization_ordering.rs::an_unauthorised_caller_cannot_tell_an_exposed_verb_from_a_withheld_one`
    and its siblings. The layered behaviour is in
    `crates/fabric-data-api/tests/layered_authorization.rs`.
- **OpenFGA is not deployed.** Platform
  `applications/core/openfga/README.md:11` says "**not deployed** — no
  `application.yaml`".
  - ADR 0016's control-plane surface is not built.
  - Nothing converges `spec.authorization` into an OpenFGA store or model
    (`docs/architecture/control-plane.md:1607-1620`: "OpenFGA ← not built").
- **Entitlements.**
  - ADR 0021 (Proposed): "Navigation is an entitlement preview; the
    application enforces its own permissions" (`docs/decisions/0021-…:591-592`).
  - ADR 0024 §3 (Accepted): `roles` is the realm tier; finer checks belong to
    the module.

### Unresolved

**U3.1: what enforces data-path authorization for a served tenant**
(**D01-10**).

| Option | Shape | For | Against |
|---|---|---|---|
| A | **Scopes and roles only** (status quo) for the first tenant; relationship-based checks later | Built and tested; no new service on the request path | Nothing enforces the relations `spec.authorization` declares; the administrator-role bypass applies |
| B | **The Data API calls `fabric-fga-auth` `Check`** after the scope check | Enforces what ADR 0013 lets clients declare | Needs OpenFGA deployed, model convergence and an API to write tuples; adds a new failure mode (503) to every request |
| C | **Layered:** A now, then B as a per-resource opt-in once OpenFGA is deployed | Incremental | Two models to explain to operators |

*Recommendation:* **C**, with A only as the posture for the isolated
disposable trial (M1) and an explicit statement that declared relations are
not yet enforced there; B enforced from M3, and for any tenant beyond the
trial on every resource that declares relations (see D01-10a). Owner:
**Brett**.

Sub-choice **D01-10a** (formerly D01-11): who deploys OpenFGA and converges
`spec.authorization` into it.
- Options:
  - **A.** The platform deploys the `fabric-openfga` image, and Fabric
    reconciles models through ADR 0016's control-plane surface.
  - **B.** Defer OpenFGA deployment until a named later gate.
- *Recommendation (not a decision):* if D01-10 is C, one schedule:
  - **Isolated disposable trial (M1):** no OpenFGA; scopes and roles only,
    with declared relations stated as unenforced. This is a trial exception,
    not a tenant-launch bar.
  - **Convergence and runtime publication (M2):** no tenant beyond the trial,
    so no change.
  - **Tenant trust and isolation (M3):** **A** — OpenFGA deployed and models
    converged, so the two-tenant proof (#116) runs against the enforcement
    that tenant launch will use.
  - Any tenant served beyond the isolated disposable trial requires D01-10a A
    (OpenFGA deployed, models converged) and the D01-10 B `Check` on every
    resource whose `spec.authorization` declares relations; under C, opt-in
    applies only to resources that declare none. Brett has not decided this
    schedule.

**U3.2: the administrator-role bypass and operator granularity** (**D01-12**).

Options:
- **A.** Keep a configurable realm-role bypass in the Data API (status quo).
- **B.** Remove `administrator_role`, or allow it to be set to none, and
  forbid `require_scopes = false` outside tests.
- **C.** Introduce operator roles finer than `fabric-operator`.

*Recommendation:* **B** for the Data API. A role from a tenant realm should not
bypass every resource check by default, and switching scopes off disables
enforcement completely. Operator granularity stays coarse until a second
operator persona exists. Owner: **Brett**.

**U3.3: where entitlements are enforced** (**D01-13**). Does Fabric enforce
plan entitlements, for example through a `/api/user/configuration` route like
the Karo reference in ADR 0024, or does it only preview them?

Options:
- **A.** Applications enforce and Fabric previews (ADR 0021's text).
- **B.** Fabric enforces at the edge or the runtime, by plan.

*Recommendation:* **A** for M4, revisited with commercial policy (#127).
Owner: **Brett**.

---

## 4. Service lifecycle

### Decided

- **The master realm's lifecycle needs no human:** ADR 0025 §5 (Accepted).
  `scripts/check.py` in the platform repository is the enforcement point.
- **Running versions come from deployment evidence:** ADR 0022 (Accepted,
  scoped to the LucentRoot readiness milestone).
  - Tests: `crates/fabric-deployment-kubernetes/src/evaluate_tests.rs::stopped_workloads_require_completed_scale_down`,
    and the others listed at `docs/verification.md:517-592`.

### Documented or built, not an Accepted ADR

- **Fabric's own components** advance automatically. They can be paused (with
  a hold) and rolled back (also with a hold). Their desired state lives in
  platform Git (`docs/architecture/control-plane.md:698-720`).
- **Deletion is not decided.** ADR 0008 lists "Deletion" under "What this does
  not decide" (`0008-…:126-131`). Today nothing in identity reconciliation
  deletes. This feeds **D01-14**.

### Proposed (built, not decided)

- **ADR 0021 §2, §4, §5 (Proposed).** The console creates clients; assignments
  are projected as public identity clients; **removing an assigned application
  is refused.**
  - Code: `crates/fabric-control-plane/src/service/set_product.rs:45-57`.
  - Test: `crates/fabric-control-plane/tests/product_workflows.rs::put_product_refuses_to_remove_an_assigned_application_and_leaves_the_client_unchanged` (:330).
- **Data source removal.**
  - Removing a data source is refused while a tenant is placed on it (the
    route list in `routes.rs`).
  - If a data source disappears anyway, the runtime fails closed:
    `crates/fabric-data-api/tests/data_source_lifecycle.rs::removing_a_data_source_makes_its_tenants_fail_closed` (:156).
- **ADR 0023 §5 (Proposed): raising the runtime is a manual one-line platform
  change** (`docs/decisions/0023-…:316-325`). It happens once three conditions
  hold:
  - the documents are published;
  - the connectors are listed;
  - the issuer registry covers every placed tenant.
- **ADR 0026 (Proposed).** Components describe themselves in an OCI
  descriptor; selecting one is an operator's act.

### Not built

- Nothing deploys application components, issues their DNS names or
  certificates, or observes their health (`README.md:468-470`).
- There is no client deletion (`README.md:463-467`).
- There is no connector deployment (see §1).

### Unresolved

**U4.1: what deprovisioning undoes** (**D01-14**). Sources: ADR 0021 owed
decision 3, ADR 0023 "Deprovisioning" and ADR 0008 "Deletion".

Options:
- **A.** Tombstone: mark removed, revoke the identity client, keep data for a
  retention window, then delete with confirmation.
- **B.** Hard delete with typed confirmation.
- **C.** Keep refusing removal (status quo). The only exit is a hand edit in
  Git.

*Recommendation:* **A**, with retention set by #127. Owner: **Brett**.

*Disposed 2026-10-11 (D01-14):* tombstone first, explicit retained state,
confirmed deletion as a separate operation. Neither revokes the identity
client nor deletes data, and no retention period is set. Both record intent
only: the tenant stays published, and withdrawal from the runtime is
deferred to its own reviewed operation. See
[Dispositions recorded](#dispositions-recorded).

**U4.2: private-network (`.internal`) clients** (**D01-15**). Sources: ADR 0021
owed decision 4; #70 "Point 3".

Options:
- **A.** A projected client follows the kind of its hosts (`privateNetwork` for
  `.internal`).
- **B.** Applications are for public hosts only.

*Recommendation:* **A**, because LucentRoot needs it. Owner: **Brett**.

*Disposed 2026-10-11 ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)); built 2026-10-12, under review:*
**A**: "Follow the declared host kind, preserving private-network
reachability. Do not expose an internal client publicly or change networking
to implement this."
- A client on `.internal` hosts only is projected as `privateNetwork`, with
  `<scheme>://<host>/<application id>/callback` per host and no public
  template callback. The scheme is declared per client
  (`spec.privateNetworkScheme`, `http` or `https`) and never inferred
  ([#108 comment 6119136505](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6119136505)).
  A stored document without one is held as `unverifiable` until a product
  save declares one.
- A client on public hosts, or none, is projected exactly as before.
- A client mixing `.internal` and public hosts is refused; loopback still is.
- **Partial:** a converged client moving between hosts is classified against a
  declaration ledger, trusted provenance of the last successful projection:
  new callbacks are written and old ones removed, a hand removal is still
  held, and with no known baseline the move is held whole until an audited
  re-assertion. No deployment has a ledger until the protected continuity
  store of [#108 comment 6119161317](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6119161317)
  exists, so every move is held today. Review 5487789024 reproduced a
  containment bypass with two concurrent reconcilers and an injected ledger:
  a late `Written` cleared another operation's pending write. Before any
  ledger adapter is enabled, these are prerequisites: operation pairing so a
  `Written` confirms only its own reservation; baseline check-and-set and
  fencing; per-identity coordination of provider writes across sweep, direct
  and re-assertion passes; stale-completion rejection; pending writes that
  survive a failed completion and a restart; and protected identity-instance
  and declaration fingerprint and freshness context. Neither O5 nor durable
  host transitions are complete.
- The D01-19b report, withhold and refusal rules apply unchanged.

Source only: no network, CA, gateway, Keycloak-scope, credential or live
change. See
ADR 0021's
[D01-15 amendment](../decisions/0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md#amendment-2026-10-12-a-projected-client-follows-the-kind-of-its-hosts-d01-15).

**U4.3: client upgrades, and what `automatic` means for application
components** (**D01-16**). Sources: ADR 0021 owed decision 7; ADR 0026 "What
this does not decide".

Options:
- **A.** Every client move is an operator save.
- **B.** A per-client policy that mirrors Fabric's own `automatic` and hold.
- **C.** Catalogue-level rollout waves.

*Recommendation:* **A** for the pilot. Owner: **Brett**.

**U4.4: who deploys an application's components.** This is part (c) of
**D01-3**, formerly D01-17. ADR 0021 and ADR 0026 both leave it open.

Options:
- **A.** Platform composition per client.
- **B.** A Fabric deployment controller with namespaced write RBAC.

*Recommendation:* **A**. Owner: **Brett**.

**U4.5: raising the runtime** (**D01-18**).

Options:
- **A.** A manual one-line change after a checklist (ADR 0023 §5 as written).
- **B.** Fabric reports a "ready to raise" gate, and a person flips it.
- **C.** Automate it.

*Recommendation:* **B**. This requires amending ADR 0023 §5 (see D01-2).
Owner: **Brett**.

**U4.6: the rest of ADR 0021's owed decisions, and accepting ADR 0021 and
ADR 0026.** Owner: **Brett**.

- **D01-19a: what "client created" means** (ADR 0021 owed decision 2,
  `0021-…:612-615`). Evidence: the README says creation writes only the
  document (`README.md:463-467`).
  - **A.** A document with a realm and a product configuration counts as
    "created".
  - **B.** Creation waits for routing, data placement and a secret boundary.
  - **C.** Creation is shown with an explicit "incomplete" state.
  - *Recommendation:* **C**.
  - *Disposed 2026-10-11:* **C**, derived from provisioning, identity and
    publication evidence. See [Dispositions recorded](#dispositions-recorded).
- **D01-19b: who owns a projected identity client** (ADR 0021 owed decision 5,
  `0021-…:626-628`). Evidence: §4's projection replaces entries by id
  (`0021-…:273-313`).
  - **A.** A product save keeps overwriting hand edits.
  - **B.** Projected clients are marked as the product's and refused in
    identity edits.
  - *Recommendation:* **B**.
  - *Disposed 2026-10-11; first slice built 2026-10-12 (#147); the owed
    follow-up built 2026-10-12 (#151, wording corrected by #153); complete:*
    **B**, extended to the identity provider.
    - An identity edit to a projected client is refused, except on a retired
      client.
    - A product save refuses a stored projected entry that diverges from the
      stored projection.
    - In Keycloak, a hand edit that only narrows a managed field is held and
      reported as `handEdited`, and an audited re-assertion overwrites it.
    - Every client read reports a stored projected entry that diverges from
      its projection. A sweep withholds it as `withheld`, and a re-assertion
      onto it is refused. See
      [Dispositions recorded](#dispositions-recorded).
- **D01-19c: activity and the document size limit** (ADR 0021 owed decision 6,
  `0021-…:629-633`). Evidence: today activity is bounded only by
  `422 document_too_large`.
  - **A.** Keep activity in desired state, with a trimming rule.
  - **B.** Move it to a durable audit store.
  - **C.** Drop it in favour of Git history.
  - *Recommendation:* **B**, with retention from #127.
  - *Disposed 2026-10-11:* activity is a bounded view, not the audit trail; a
    durable audit source is designed with integrity and size tests, and
    nothing prunes. See [Dispositions recorded](#dispositions-recorded).
- **D01-19d: accept ADR 0021 and ADR 0026.**
  - Accepting ADR 0026 partly accepts ADR 0023 (`0026-…:18-20`), so it comes
    after D01-2.
  - ADR 0026's six owed decisions are at `0026-…:909-935`: images without a
    descriptor, browsing, declared fields and plans, adoption, defaults, and
    the free-text kinds.
  - **A.** Accept each after its owed decisions.
  - **B.** Accept now and record the owed decisions as follow-ups.
  - *Recommendation:* **A**, in order: 0023, then 0021, then 0026.

---

## 5. Rollback boundaries

### Decided

| Boundary | Rule | Source |
|---|---|---|
| Configuration across the observer boundary | Older binaries reject the new configuration, so rolling back across that boundary must remove the configuration along with the image pins | ADR 0022 "Rollout" (Accepted) |
| Master instance | Apply is idempotent and its drift check is the proof. Lost OpenTofu state is recovered by adopting existing objects. `prevent_destroy` guards against deleting a client. Rotating the gateway secret means regenerating it and re-running | ADR 0025 Consequences (Accepted) |

### Documented or built, not an Accepted ADR

| Boundary | Rule | Source |
|---|---|---|
| Fabric's own component (images) | Rolls back to an older published version, restoring the version **and** the exact bytes. Writes a hold (`reason: rollback`) in the same commit. A request carrying a digest is refused | `docs/architecture/control-plane.md:722-780, 960-1030`; `crates/fabric-control-plane/tests/platform_integration.rs::a_rollback_carrying_a_digest_is_refused_rather_than_ignored` (:200); `platform_described.rs::a_described_rollback_candidate_names_its_commit_and_no_digest` (:137) |
| Fabric's own component (chart) | Restores the version, but not provably the bytes. The difference is stated, not enforced | same document, `:744-760` |
| Runtime documents | The revision is monotonic, and an older revision is refused (§6). A breaking change ships under new file names beside the old ones (§9). Deprovisioning is an empty set inside a document, never a delete (`0018-…:616-617`) | ADR 0018 (Proposed); tests in §1 |
| Wave ordering | Holds only when an environment is **created**. On an existing environment the child Applications sync independently | PR93 and PR45 (both open, not merged); observed on 2026-09-22 according to PR45's text, **not re-verified** |
| Identity reconciliation | Adds and corrects. Deletion is not decided (ADR 0008 "What this does not decide"; see D01-14) | ADR 0008 (Accepted) |

### Unresolved

**U5.1: ordering updates, not only creation** (**D01-20**). The mechanism under
discussion is platform #44, `RollingSync`.

Options:
- **A.** Adopt `RollingSync`.
- **B.** Accept creation-only ordering, and rely on a loud `Degraded` plus a
  manual revert of the OIDC policy.
- **C.** Add a pre-sync health check in wave 40.

*Recommendation:* **B** for the disposable trial, then **A** before the durable
pilot. Owner: **Brett**. **Not verified:** the state of platform #44.

**U5.2: a rollback floor for Fabric's own component across a format change**
(**D01-21**). ADR 0026 lists this under "What this does not decide".

Options:
- **A.** Record a minimum rollable version per release, and refuse rollback
  below it.
- **B.** Document the floor only.
- **C.** Only fix forward.

*Recommendation:* **A**, as part of the release compatibility manifest (#124).
Owner: **Brett**.

**Disposition (2026-10-11, D01-21, [#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): A.**
Record and enforce a minimum rollable version in the compatibility manifest,
with format and migration compatibility tests. Built: `compatibility.yaml`
sets `minimumRollableVersion: 0.3.0-preview.16`. Platform Management refuses
an in-product rollback of Fabric below it, and the publication contract is
raised to 2 so publishers from #136 to #138 refuse what this one writes. A
break-glass edit is not refused, and nothing is verified live
([gap report](m2-publication-gap-report.md#release-rollback-floor-d01-21)).

**U5.3: rolling back runtime documents.** This is now part of **D01-1**
(formerly D01-22). Revisions cannot go backwards.

Options:
- **A.** Roll forward only: republish the earlier content at a new revision.
  This matches the verdict table as built.
- **B.** Allow an operator to reset the revision.
- **C.** Revert `placements.yaml` and `data-sources.yaml` in Git and let the
  publisher republish.

**Disposition (2026-10-11, D01-1, [#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): A, roll forward
only, without C as the mechanism.** D01-27 rules out a blind revert of
`placements.yaml`, and data sources change only through writes that bump
their revision. Recorded in ADR 0018 part 10.

*Recommendation (superseded by the disposition above):* **A**, using C as the mechanism. Record it in ADR 0018 when
it is accepted.

**U5.4: tenant data rollback and schema migration** (**D01-23**). Nothing
creates tables (ADR 0023), and off-host recovery is unproven (#108's evidence
limits; platform #50).

Options:
- **A.** The platform owns backup and restore for each data source.
- **B.** Fabric records restore points beside placements.
- **C.** Out of scope until the durable pilot.

*Recommendation:* **A**, gated to M5. Owner: **Brett**.

**Other gaps, each owned by Brett:**
- **D01-25: lifecycle and rollback of client secret versions.** ADR 0017
  (Accepted) decides where a secret boundary is and who may reach it. It says
  nothing about versions beyond recording the version on reveal
  (`docs/decisions/0017-…:94-97`). Draft PR105 and PR106 touch secret versions
  in the console (version check-and-set, metadata read).

  What exists today:
  - Every write sends a check-and-set (`cas`) version, and `None` is sent as
    `0`, so a write to an existing secret without its version is refused
    (`crates/fabric-openbao/src/client_secrets/operations.rs:53-60`). A
    mismatch is `SecretsError::Conflict` (`crates/fabric-openbao/src/client_secrets/wire.rs:60-66`,
    `crates/fabric-control-plane/src/client_secrets/errors.rs:21-28`).
    The handler reads a body field `expectedVersion`
    (`handlers/secrets/write.rs:17,29`), while
    `docs/architecture/control-plane.md:149` describes it as an optional
    `If-Match`. That is doc drift.
  - Delete goes to the metadata endpoint and removes **every** version,
    irreversibly (`operations.rs:69-83`; `control-plane.md:150`). There is no
    per-version delete, undelete or destroy.
  - There is no rollback path. `ClientSecrets` has list, metadata, reveal,
    write and delete only (`crates/fabric-control-plane/src/client_secrets.rs:52`).
    Reveal takes a path and no version (`handlers/secrets/reveal.rs:21-24`), and
    metadata reports only the current version and its time
    (`crates/fabric-control-plane/src/client_secrets/values.rs:14-22`). An older value cannot be read through
    Fabric.
  - Retention is unconfigured. No `max_versions` or `delete_version_after` is
    set in either repository, so the store's default applies. **Not verified:**
    the default on a client namespace's `secret/` mount (OpenBao's documented
    KV v2 default is 10), and where that mount is created.
  - The platform policy `+/secret/*` with `update` (ADR 0017:20) appears to
    cover KV v2's undelete and destroy paths. **Not verified** against a real
    store.

  Options:
  - **A.** Roll forward only. Rollback is writing the earlier value as a new
    version, under the existing check-and-set. No API change. Against: the
    operator needs the old value from outside Fabric, because Fabric cannot
    show it.
  - **B.** Add a version read and a restore. Reveal takes an optional version,
    and restore reads version N and writes it as a new head under check-and-set.
    Both are audited like any reveal. Nothing is undeleted or mutated in
    place. For: a real rollback. Against: it widens the reveal surface to
    history, and needs OpenAPI, console and audit work.
  - **C.** Retention policy. The platform sets `max_versions` (and optionally
    `delete_version_after`) on the mount, and Fabric adds an explicit destroy of
    old versions. For: bounded history and a deliberate way to remove a leaked
    value. Against: destroy is irreversible, and a low cap shortens the
    rollback window in B.

  *Recommendation (superseded by the disposition below):* **A** now, with C's
  retention setting made explicit in the platform and no destroy endpoint yet.
  Take **B** when an operator needs to read an older version.

  **Disposition (2026-10-11, D01-25, [#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): A.**
  Roll forward under the existing check-and-set only; no historical reveal,
  undelete or destroy endpoint. Retention is documented explicitly, and its
  actual limits are proposed separately before a live mount changes or retained
  history is shortened. Recorded in ADR 0017's amendment of 2026-10-12, which
  also corrects the facts above as of `main` `4048c6a`:
  - PR105 and PR106 merged (`04f9276`, `bb864f4`).
  - The `If-Match` drift is fixed: `control-plane.md` now says `expectedVersion`
    in the body.
  - The audit record's `version` is filled by a write only. A reveal records
    none, so ADR 0017's "recording the version on reveal" was not true of the
    code; the ADR now says so.
  - Retention is measured on the fixture, not verified live: a mount created
    with no options keeps the newest 10 versions with no age limit, and the
    `+/secret/*` policy reaches older versions, undelete, destroy and the
    mount's `config`. Fabric's adapter reaches none of them, and
    `scripts/check_architecture.py` holds it there.
  - Still **not verified**: where a live client mount is created and with what
    options. LucentRoot's control-plane policy at platform `main` `4152687`
    grants no client namespace at all.
- **D01-27: rollback of the desired-state repositories.** This covers
  `saas-fabric-clients` (the catalogue and client documents) and the platform's
  `data-sources.yaml` and `placements.yaml`. Is a Git revert a supported
  operator action, given conditional writes and activity entries? Not decided.
  A Git revert is a proposed rollback, and its effect on live state is
  **unverified**: nothing here has been run.

  What exists today:
  - Writes are conditional. Client and catalogue writes state the revision
    they edit (`crates/fabric-client-git/src/repository.rs:70,89`), and the
    contents API call carries it as `sha`
    (`crates/fabric-client-git/src/github/operations.rs:60-70`). A stale write
    is `409 revision_conflict` and a missing one `428 revision_required`
    (`docs/architecture/control-plane.md:241-242`, `:1083`). ADR 0008 decision
    4 says there is no last-writer-wins path (`docs/decisions/0008-…:54`).
  - Activity entries live inside the same document and are appended by
    operator writes (ADR 0021 §6, `:330-348`). ADR 0021 calls activity "a
    view, not the audit trail" (`:348`). A revert of a commit takes its
    activity entry with it, and the control plane's own audit event is not
    emitted for a Git-side change (inference from ADR 0008:81-87, not tested).
  - Placement `revision` is per record, and a tenant's published binding
    revision is the **sum** of its records' revisions
    (`crates/fabric-platform-management/src/placements/record.rs:48-70`;
    `src/publication/snapshot.rs:78-107`). ADR 0018 (Proposed) says a tenant
    revision "only ever increases" (`:407`) and the runtime ignores an older
    one (`:46`). A revert that lowers a record's revision lowers the sum, so
    the runtime would keep serving the pre-revert binding. A revert that
    leaves the sum unchanged but the bytes different would be published
    (the publisher answers `DivergentPayload` by bumping the document
    revision, `publication/protocol.rs:46-51`), but the runtime's per-resource
    rule (ADR 0018:46-47) would still refuse the tenant's changed binding at an
    unchanged revision. **Not verified** by a test.
  - Publication advances a document revision only when the bytes differ. It
    offers the held revision and bumps only on `DivergentPayload`
    (`publication/protocol.rs:46-51`; ADR 0018 :270-273). Rollback of runtime
    documents is roll forward, per U5.3 and ADR 0018 :343.
  - Reconciliation is additive and deletes nothing (ADR 0008 decision 3,
    `:50-51`), so reverting a client document does not remove what an earlier
    reconcile created.
  - `record.rs` says a placement must not be removed by hand until
    deprovisioning exists (D01-14). Reverting the commit that placed a tenant
    does exactly that.
  - An erroneous application assignment has no exit in the product: ADR 0021
    §5 (Proposed, `0021-…:327-328`) says the only one is a hand edit in Git.
    Nothing in the catalogue can be deleted either (§5). Verified at `4ed4088`:
    no restore, history or revision route exists in
    `crates/fabric-control-plane/src/routes.rs`.
  - ADR 0023 (Proposed, `0023-…:87-88`) says editing `data-sources.yaml` by
    hand remains the break-glass path and is expected to keep working.
    `environments/README.md` at platform `0ff5d66` says the same for
    `placements.yaml`. ADR 0023 `:205-211` forbids hand removal of a placement
    record.
  - Platform `main` (`0ff5d66`) has no `data-sources.yaml` or `placements.yaml`
    under `environments/`. Only `environments/README.md:190-266` documents
    them, so there is no deployed file to revert today.

  Options:
  - **A.** A Git revert is a supported operator action. The reconciler and
    publisher treat the reverted content as new desired state. For: simplest,
    and Git already records it. Against: the lowered placement revision and
    dropped activity above, no audit event, and nothing undoes what
    reconciliation already created.
  - **B.** Revert only through the console, as a new write at the current
    revision. Git history is never rewound by hand. For: keeps conditional
    writes, activity and the audit event, and moves revisions forward. Against:
    the console has no restore or history action (verified at `4ed4088`: no
    restore or history route exists), placements have no console edit beyond
    placing, and an erroneous application assignment has no console exit
    (ADR 0021 §5).
  - **C.** Forbid revert of `placements.yaml` and of data sources that a
    placement names. Allow a revert of the catalogue and client documents
    (through B, with Git revert as break-glass). For: protects the one part
    where a revert can strand tenant data. Against: a convention unless
    `scripts/check.py` or branch protection enforces it (**not verified**).

  *Recommendation:* **C**: placements roll forward only (ADR 0023 already
  forbids hand removal), and data sources only by a revision-bumping edit, by
  the console's `PUT` or by break-glass per ADR 0023. Catalogue and client
  documents keep the Git edit as break-glass per ADR 0021 §5, until a console
  restore exists. Decide with D01-1, D01-2 and D01-19d (acceptance of ADR 0021
  and ADR 0023). Make no Git revert claim until a test shows what the
  reconciler and runtime do with it. Owner: **Brett**.

  **Disposition (2026-10-11, [#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)).**
  - Placements roll forward only.
  - Referenced data sources change only through writes that bump their
    revision.
  - No blind placement revert or removal.
  - Restoring the catalogue or a client must preserve conditional writes and
    audit evidence.
  - Any break-glass Git effect is characterized in tests before it is
    claimed as a supported operational rollback.

  Recorded in ADR 0018 part 10 and ADR 0023's amendment. The owed items are
  O1, O3, O4 and O5. O1 is implemented in #136. O3 and O4 are implemented,
  and O5 is partly characterized, in the change that follows #136. O5 is
  characterized for the catalogue and client documents in #159, with
  findings open on #108 (none of it verified live).

---

## PR state revalidated

| | saas-fabric PR #93 | saas-fabric-platform PR #45 |
|---|---|---|
| Head | `647ef8d` (1 commit, 1 file, +20/−3) | `fd12327` (1 commit on `355fe6d`, 5 files, +110/−34) |
| Draft or ready | Ready (not draft), open | **Not verified.** GitHub API access to this repository is not enabled for this session |
| Mergeability | GitHub reports `mergeable_state: clean`. A local `git merge-tree` against `1f46788` is clean. `main` has moved 7 commits (#94–#100) since the base `8676af4`, and none touch ADR 0025 | A local `git merge-tree` against `0ff5d66` is clean. `main` has moved 2 commits (#46, #47) since `355fe6d`. #46 changed `scripts/check.py` by +621/−9 and also edited `saas-fabric-control-plane/README.md`, which PR45 edits too. A clean textual merge does not mean the checks pass. **Not verified:** GitHub's mergeability |
| CI | 24 check runs, all `success`, on `647ef8d` (2026-09-22). They are two runs of the same workflow (9 jobs each), five CodeQL analyses and the CodeQL aggregate check. Not re-run against current `main` | **Not verified** |
| What it changes | Amends **Accepted ADR 0025 §1**: the wave gate holds when an environment is created, not on every update, and platform #44 tracks ordering updates. It also adds an interim rule to "What this does not decide": the bootstrap administrator is the declared operator until account origin is decided, with Karo's brokering cited as the reference. The status stays Accepted | Changes the roster from `["brett@fieldstate.nz"]` to `["admin"]`, which changes what the convergence grants. The READMEs now say when the wave gate holds and record brokering as the open question |

### PR dispositions (formerly D01-24)

**P-1. Neither PR merges under #108.**

1. Brett decides the interim for D01-6 first.
2. PR45 then merges only as a separately authorised change to LucentRoot,
   because merging it is a deployment:
   - LucentRoot tracks platform `main`
     (`environments/lucentroot/kustomization.yaml:37-52`).
   - The `master-instance` Job re-runs on every sync with the bootstrap
     administrator's credential.
3. Before PR45 merges, three things must be in place:
   - platform #48's diagnosis;
   - CI passing on PR45's exact head after rebasing onto `0ff5d66`;
   - a stated rollback: revert to the earlier roster, or to `[]`. A Git
     revert is a proposed rollback; that it restores the effective master-realm
     roles is unverified, because the convergence grants without revoking
     (D01-6b).
4. PR93 merges in the same window. Its new ADR 0025 text says the bootstrap
   administrator is the declared operator, which describes PR45's roster and
   is not true of platform `main` until PR45 lands. This requires Brett to
   accept that amendment and interim as part of D01-6.

---

## Dispositions recorded

IngeniousRiley recorded these on 2026-10-11 under Brett's delegated decision
authority ([#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)). This section covers only the dispositions
this document's ADR amendments act on. The comment holds the others.

| ID | Disposition | Recorded in |
|---|---|---|
| D01-1 | Amend ADR 0018 to the verified contract and accept it through the reviewed change. Roll forward only: earlier content becomes a new revision, never a revision reset. Tenant and resource revisions stay monotonic, as well as document revisions | ADR 0018 status, part 10, "Owed" |
| D01-2 | Amend ADR 0023 before acceptance. Taking an already-declared compatible dedicated source is distinct from provisioning a new one, and no automatic provisioning is claimed. Platform wiring and control-plane data ownership are preserved, stale console documentation is fixed (`README.md`, `docs/architecture/control-plane.md`), and activation stays explicit and readiness-gated. #112's remaining gaps go to bounded PRs | ADR 0023 status, part 2, part 4, part 5, amendment, "Owed" |
| D01-27 | Placements roll forward only. Referenced data sources change through revision-bumping writes. No blind placement revert or removal. Catalogue and client restoration keeps conditional writes and audit evidence. Break-glass Git effects are characterized in tests before they are claimed as rollback | ADR 0018 part 10; ADR 0023 amendment |
| D01-19d | Reviewed amendments and acceptance go in order: 0023, then 0021, then 0026. Each owed item is reconciled with the disposition, and an unimplemented guarantee is never marked verified just because an ADR is accepted | ADR 0023 status |
| D01-5 | Keep ADR 0019's single-runtime-route model and the platform-owned G1–G17 edge. Client UI routes add no alternate unguarded `/v1/data` path. Prepare the acceptance and implementation tests. Live JWT and trust configuration stays behind the separately scoped security gate | ADR 0019 status, amendment 2026-10-12 ("client routes"), §G1; `identity-edge-test-matrix.md` A9, A10a, D4, D12 |
| D01-14 | Tombstone first, explicit retained state, confirmed deletion as a separate operation. No destructive action or retention period is approved. State machine and refusal tests; retention and customer-data choices surfaced separately. Withdrawal from the runtime is deferred | ADR 0021 amendment 2026-10-11 ("The rules", "Withdrawal is deferred", "Deferred, and how each is bounded") |
| D01-19a | Explicit incomplete state until provisioning, identity and publication readiness is met; saved intent is distinguished from a usable client | ADR 0021 amendment 2026-10-11 ("The states") |
| D01-10, D01-10a | Layered authorization. Scopes and roles only for the isolated synthetic trial; OpenFGA Check on relation-declaring resources from M3 and for every tenant beyond the trial. Designed and tested now; no deployment or standing access. Collections (item 1 of the follow-up): a distinct per-resource collection object with `can_list`/`can_create`, bound to the tenant by the token-selected store. Spelling, model writer and model shape disposed in #108 comment 6118398377. Built 2026-10-12, with a required CI proof against pinned OpenFGA | ADR 0027 (Proposed) §1–§4, §2a (amended 2026-10-12), §6; "Owed" |
| D01-12 | No data-path administrator bypass; scopes cannot be disabled outside tests; coarse platform-operator controls retained within their current boundary | ADR 0027 §5 |
| D01-9 | Retire the loopback workbench and `/api/session` only after the disposable test and runbook move to the gateway flow or the profile is explicitly parked; keep local automated tests possible; do not silently invalidate #101/#111. Parked, then removed | ADR 0021 amendment 2026-10-12; ADR 0024 "Implementation status"; ADR 0010 (superseded flow); `examples/disposable-dogfood/README.md` "Parked (D01-9)"; U2.6 above |
| D01-19c | Activity is a bounded view, not the audit trail. A durable audit source with integrity and size tests; authoritative history is never silently pruned. Retention, storage activation and customer-data commitments stay separate | ADR 0028 (Proposed) §1–§5; "Deferred, and not decided here" |
| D01-19b | Product-owned projection; unsupported hand edits are refused instead of silently fought; current permission boundaries are preserved | ADR 0021 amendment 2026-10-12, D01-19b ("Two places…", "The drift state", "What an operator does", "Known limits") |
| D01-25 | Roll forward under the existing check-and-set only; no historical reveal, undelete or destroy endpoint. Retention documented explicitly; its actual limits proposed separately before a live mount changes or retained history is shortened | ADR 0017 amendment 2026-10-12, sections 1–4 |

**D01-19b: what is built and what stays open.** D01-19b is complete: the
first slice merged in [#147](https://github.com/FieldstateNZ/saas-fabric/pull/147),
the owed follow-up in [#151](https://github.com/FieldstateNZ/saas-fabric/pull/151),
and [#153](https://github.com/FieldstateNZ/saas-fabric/pull/153) corrected
the missing-entry limit's wording. First slice:
- for the fields the adapter observes, product-owned drift corrected and
  narrowing hand edits held. This split is accepted in the
  [engineering disposition](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6116879549).
  Unread fields are unmanaged and not evaluated, not attested safe.
- the `handEdited` status and typed `handEdits` in the API and the console;
- the audited `POST /api/clients/{clientId}/identity/reassertion`, whose
  record keeps confirmed, unconfirmed and counted effects of a partial
  failure;
- the refusal of an identity edit to a projected client on a non-retired
  client, except one that restores the projection;
- the pre-write guard: a product save refuses a stored projected entry that
  diverges from the stored document's own projection.

No permission, credential or service-account scope changed.

The owed follow-up, ADR 0021's "Owed before D01-19b is complete", is
built and merged. Its defaults were asked on
[#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6117200254)
and accepted in the
[engineering disposition](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6117388945),
for source design only:
- read-time reporting: `projection` (`consistent`, `diverged` with each
  `{client, field}`, or `unverifiable`) on `GET /api/clients`,
  `GET /api/clients/{clientId}` and `GET /api/clients/{clientId}/identity`,
  and in the console. It reports a stored inconsistency. It does not say who
  edited Git.
- refusing projection-based convergence: a sweep withholds each diverged or
  unverifiable projected entry. The entry is neither written nor corrected,
  and the client is reported as the new `withheld` status. Identity
  readiness needs a consistent stored projection: `withheld`, and any
  `diverged` or `unverifiable` projection even with nothing withheld (a
  missing entry, say), leave it unmet. Nothing already published, no session and no provider
  access is revoked.
- refusing re-assertion: a re-assertion onto a diverged entry answers `409
  projected_client_diverged` before any provider call, and is not audited.

What stays open is what ADR 0021's D01-19b amendment records under
"Known limits": a withheld client is not corrected while its entry diverges;
a missing projected entry is reported and held from readiness, not withheld,
and that is not a frozen generation or proof of revocation; a substituted
callback leaves a client with none until someone re-asserts; status is per
process; and unmanaged fields are not evaluated. Neither owed item added a
forced repair or an automatic credential or access change.

**D01-14 and D01-19a: what stays open.** Built: the state machine, its
refusals, the API and console state, and `lifecycle.withdrawal`, which reports
a retired tenant as `pending`, `refused`, `unobserved` or `notPublished` from
publication evidence only.

**Withdrawal is deferred.** A tombstone does not take the tenant out of the
runtime documents. Filtering it out before composition would remove a
published binding without a revision bump, which #138's history guards
refuse; in a multi-tenant environment that stalls publication with the
retired binding still published. Withdrawal needs its own reviewed slice: a
revision-bumping explicit retirement those guards can admit, with two-tenant
and final-tenant proofs, restart and held-record tests, and reader-applied
evidence before anything claims a tenant stopped being served.

Engineering bounds for the rest come from
[#108 comment 6113749198](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6113749198):
no automatic deletion; a reversible-restore slice with CAS and retained
history, built as source and held until the protected continuity store exists
(ADR 0021's restore amendment); no live identity, session or credential change; and
two-step, revision-fenced, audited confirmation under existing authorization.
Still user-only: the retention duration and export obligations, the scope and
execution of eventual deletion, and any live identity or session change. ADR
0021's amendment lists each under "Deferred, and how each is bounded".

**G7: reader-generation evidence.** Asked on
[#108 comment 6117881889](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6117881889);
disposed for source design and preparation only in
[#108 comment 6118073758](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6118073758).
Built in source in [#157](https://github.com/FieldstateNZ/saas-fabric/pull/157);
not wired, not approved for live use and not verified live (see the last
bullet).
- **Option A is chosen:** "the control plane pulls a minimal per-pod report
  through the existing read-only Deployment/ReplicaSet/Pod observer
  boundary." No runtime write RBAC, `TokenReview`, standing human or
  platform-admin bearer, or push credential. The probe uses a separate HTTP
  client with no Kubernetes credentials, no redirects, and a bounded timeout
  and body. Its targets come only from configured Deployments and their
  API-observed owned pods, never from a URL in client desired state.
- **Source-only preparation is authorised now:** a versioned metadata DTO, a
  served-snapshot accessor, a disabled-by-default separate-listener and
  router factory, the observer and probe abstraction, aggregation, UI and
  fake-pod tests. The report carries only the served generation,
  `record_sha256` and origin from one installed, serving snapshot. The
  admin-gated `/ready` detail and the minimal body's confidentiality are
  unchanged; no metadata route goes on the main data listener; omitted
  configuration binds nothing.
- **Live use of the separate port is not approved:** "The unauthenticated
  separate-port trust model is NOT approved for live use here." Before it is
  enabled, the caller and target selectors, port, observer target and
  NetworkPolicies go for explicit security and deployment approval.
  NetworkPolicy is traffic isolation, not server attestation. No policy,
  target, listener activation, credential or grant is authorised, and
  unsupported or unconfigured probes report `unobserved`. This does not
  permit changing the current preview.15 configuration.
- **Applied:** readers owned by every ReplicaSet of the configured
  Deployment count, not only the current one. Every possible serving reader
  in one sample must report the exact expected generation and record digest
  with `published` origin. Any terminating owned pod prevents applied until
  it is gone or an existing approved mechanism proves it drained. With
  desired replicas above 0, a current Deployment observation and a nonempty
  reader set are required; missing or not-ready expected replicas never make
  a vacuous match applied. Older or `last_known_good` is `pending`; ahead,
  wrong digest, a malformed, missing or failed report, or membership or UID
  changing during the sample is `unobserved`; desired replicas 0 is
  `no_readers`, never applied and not withdrawal proof. It is
  serving-snapshot evidence, not a guarantee about later requests or token
  revocation.
- **The ADR 0023 `documents` gate stays publisher evidence.** Reader
  evidence sits beside it. Once readers are deliberately running,
  readiness-gated activation, update completion and any withdrawal-applied
  claim require applied-reader evidence. No automatic scaling, forced repair
  or new rollback authority.
- **Built in source (#157).** `fabric-core`'s four-key
  `ReaderGenerationReport`, taken from the installed resolution view
  (`RuntimeResolver::served_generation`) and served by `fabric-api`'s
  off-by-default `reader_generation_listen` on `GET /reader-generation`
  only. Every `PublicationReport` stamps the record it leaves held. The
  read-only `KubernetesReaderObserver`, configured by an optional
  `[platform_management.publication.readers]` target, samples every
  replica set's pods with an anonymous, bounded, non-redirecting probe.
  `readers::assess` answers `applied`, `pending`, `unobserved` or
  `noReaders`, and the readiness row and console show it beside the
  documents gate with `updateComplete`. No environment configures a target,
  so every environment reports `unobserved`. Still owed for live use: the
  selectors, port, read grants and NetworkPolicies above, for review. No
  withdrawal-applied claim exists, because withdrawal is not built. See
  [`m2-publication-gap-report.md`](m2-publication-gap-report.md) "Reader
  generation reporting (G7)".

**G3: publication completeness.** Asked on
[#108 comment 6119681433](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6119681433);
disposed in
[#108 comment 6119949193](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6119949193)
as the six proposed defaults, with completeness inspected at the adapter
rather than inferred from revisions. Built in source; not verified live.
- **Observed target state.** It is independent of the last pass's result.
  The answer is `complete`, `incomplete`, `unpublished` or `unobserved`,
  from one inspection that reads the record before and after every part it
  names.
- **The documents gate.** Only `complete` at exactly the accepted record
  (generation and digest) can meet it, per review 5488114355 on #163. A
  target complete at another record keeps the row `complete` and the gate
  unmet. Every other state makes the gate unmet and `updateComplete`
  false, with a sanitised reason.
- **When it is inspected, and when it clears.** It is inspected in every
  pass, including the exhausted-retry exit, and never on `GET`. A restart
  starts with nothing observed. Only a verified `complete` clears
  `incomplete`.
- **The retry is unchanged.** #132's two immediate `Unwritable` re-offers
  stand.
- **Kept separate.** This is separate from #158's callback containment and
  from O5's continuity and issuance guards.

See [`m2-publication-gap-report.md`](m2-publication-gap-report.md)
"Publication completeness (G3)".

**Owed after acceptance.** These are not built at the base of the amendment.
Each is tracked against [#112](https://github.com/FieldstateNZ/saas-fabric/issues/112).
[#136](https://github.com/FieldstateNZ/saas-fabric/pull/136) implements O1 and
O2 and part of O6. Its rows below say what remains. None of it is verified
live.

| ID | Owed | Evidence |
|---|---|---|
| O1 | **Implemented in #136.** The publisher now refuses a held tenant or data source offered at a lower `BindingRevision`, or changed at the same revision, as `ResourceRevisionNotAdvanced`. A resource absent from the held document is not compared (see O3, O4) | `fabric-runtime-publication/src/resource_revisions.rs`; `plan_tests.rs::a_held_tenant_offered_at_an_older_revision_is_refused`, `::a_held_data_source_changed_at_the_same_revision_is_refused` |
| O2 | **Implemented in #136 while the publication record survives.** A lost manifest is held at the revision the record names, so an older offer is refused as `StaleRevision`. If the whole tenants `ConfigMap` is lost, the record goes with it, and the document is still treated as a first publication | `fabric-runtime-publication/src/plan/held.rs`; `plan_tests.rs::a_lost_manifest_is_held_at_the_revision_the_record_names_so_an_older_offer_is_refused` |
| O3 | **Implemented, not verified live.** The publication record carries `resources`, the highest revision each tenant binding and DataSource was ever published at. The publisher refuses a resource absent from the held document that is offered at or below that revision (`ResourceRevisionNotAdvanced`). A declaration is written above the published revision. History starts with the first record that carries it. The platform contract must accept the new key before activation | `fabric-runtime-publication/src/resource_history.rs`, `resource_revisions.rs`; `fabric-platform-management/src/data_sources/plan.rs`; `plan_tests.rs::a_data_source_retired_and_declared_again_at_or_below_its_last_revision_is_refused`; `data_sources/plan_tests.rs::an_id_removed_and_declared_again_starts_above_the_revision_it_was_last_published_at` |
| O4 | **Implemented, not verified live.** `compose` refuses a published tenant or logical binding with no record left (`PlacementRemoved`), and records that no longer reach the published revision or that reach it with a changed binding (`PlacementLowered`). The publisher refuses a dropped tenant (`TenantRemoved`) unless the whole document is emptied with intent. Not detected: one record lowered while another of the same tenant is raised in the same write | `publication/placement_history.rs`; `snapshot_tests.rs`; `plan_tests.rs::a_held_tenant_dropped_from_the_offer_is_refused` |
| O5 | **Characterized, not verified live; no Git revert is claimed as a rollback.** Reverts of `placements.yaml` and `data-sources.yaml` are refused and leave every published file unchanged. A catalogue revert is not refused: the older definition is published forward at the next catalogue revision, and a revert leaving no release makes the pass wait. A client revert reaches the published documents only through the issuer registry. A console tag read before either revert is refused, and a console restore is conditional and audited. The revert itself is never audited. The Git adapter's revision is the blob's hash, so a revert to identical bytes makes an earlier tag current again. Findings open on #108: a freed release number is issued again for another definition; a tombstone, an application assignment or a client's creation can be undone in Git, which the console refuses, with no audit record | `fabric-control-plane/tests/platform_publication_revert.rs`, `catalogue_revert.rs`, `client_document_revert.rs`; `fabric-client-git/tests/desired_state_repository.rs::a_revert_to_identical_bytes_brings_the_earlier_revision_back_and_a_write_held_at_it_is_accepted`, `::a_catalogue_revert_brings_the_earlier_revision_back_and_a_write_held_at_it_is_accepted` |
| O6 | #112 gaps. #136 makes each process resolve a request against one generation's tenants and data sources (G1b, with a deterministic isolation test), keeps readers off a retirement until its unbinding (G2) and off an interrupted publication (G3), and gives G6 a reader-floor handshake with N/N+1 tests. G8 is partly addressed after #136: an entry that no tenant binds is refused, the controller keeps the last published catalogue, and a runtime takes its catalogue from the same read and record as the generation it primes. G1b across replicas is reported in source since G7 (#157): a replica's served generation, sampled by the control plane and held to the publisher's record stamp. G3's "incomplete" on the panel is built in source since the G3 disposition on #108 (see "G3: publication completeness" above). Still owed: G2's publisher waiting for readers; G4 (`Lease`); G5's publisher hold and publisher-side recovery (the reader's persisted last-known-good, its hold and fail-closed rules and its recovery command are built, off by default, and wait on a platform reader mount); G6 (dual-key path, per-row tests, CI check on shape changes); G7's live wiring (built in source in #157, not approved for live use; see "G7: reader-generation evidence" above); the rest of G8 (an entry that some tenant does not bind, catalogue refresh in a running replica, the Data tab) | [`m2-publication-gap-report.md`](m2-publication-gap-report.md) "Still deferred" |
| O7 | The controller does not attempt placement on each pass. Placement happens only on an explicit `POST` | `fabric-control-plane/src/handlers/placements/post.rs` is the only production caller of `Placements::place` |
| O8 | **Publisher half implemented, not verified live (D01-4, D01-18).** The readiness report names three gates: documents, identity and connectors. The issuer registry is published with the tenants document, generation-stamped and named by the publication record. Still owed: the platform contract accepts the carrier, edge and runtime apply one generation with applied evidence. Pending identity evidence excludes a client, fail closed (#108 option (a)) | `fabric-runtime-publication/src/registry.rs`; `fabric-platform-management/src/publication/readiness.rs`; `fabric-control-plane/src/service/issuer_registry_source.rs`; ADR 0018 "The issuer registry" |

## Decisions owed

Brett owns every policy decision; D01-26 is engineering ownership under ADR
0024 and needs Brett only if it changes D01-3(b). "Rec." is a recommendation
only, except where a row is marked **Disposed**: those are recorded under
[Dispositions recorded](#dispositions-recorded), and the #108 comment linked
there gives the rest. IDs merged during review are noted, and the original
IDs are not reused.

| ID | Area | Decision | Rec. |
|---|---|---|---|
| D01-1 | Publication | Accept or amend ADR 0018 (built, Proposed), including runtime-document rollback (U5.3, formerly D01-22) | **Disposed 2026-10-11:** amend and accept; roll forward only, resource revisions monotonic. See [Dispositions recorded](#dispositions-recorded) |
| D01-2 | Publication | Accept or amend ADR 0023: resolve the non-shared placement contradiction (`:222-228`/`:370-372` vs `:230-235`, implemented by `select.rs:44-47`); §5 against D01-18; fix the stale `README.md:451-454` | **Disposed 2026-10-11:** amend, then accept. See [Dispositions recorded](#dispositions-recorded) |
| D01-3 | Provisioning | Per-client platform composition, three sub-choices: (a) databases, schemas and connector processes; (b) client instance resources (formerly D01-8); (c) application component deployment (formerly D01-17) | Platform composition per client for all three. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Platform composition per client/instance; Fabric owns desired state and reconciliation under ADR 0012/0024/0025. No new standing provider-admin credential in the product. Author the composition contract and synthetic fixtures without creating credentials, activating registrations or touching other environments." |
| D01-4 | Identity | Generator for the issuer-to-tenant registry (ADR 0019 §G4a) | Fabric publisher, one generator; gateway sub-choice open. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "One canonical generated tenant/issuer generation; Fabric produces the runtime registry, platform composition owns edge application. Do not widen Fabric's live RBAC to write gateway policy. Propose a version-bound handoff that refuses mismatched registry/edge generations and prove the G4a consistency requirement before activation; this is an engineering design obligation, not permission to weaken it." |
| D01-5 | Identity | Accept ADR 0019; the platform owns the §G edge; clarify the client-route comment | **Disposed 2026-10-11:** accept as written; the platform owns G1–G17; client UI routes add no `/v1/data` path. Accepted as amended, runtime half tested. Browser data path owed to ADR 0024 slice 5. See [Dispositions recorded](#dispositions-recorded) |
| D01-6 | Identity | How operator master-realm accounts come to exist (broker / create / `admin` only / `[]`); sub-choice 6b, revocation (formerly D01-7); accepting PR93's interim | `admin` interim, brokering as target; Brett names the upstream |
| D01-9 | Session | Keep or remove the loopback workbench as `/api/session` retires (ADR 0021 owed 1); sequence with PR101 | **Disposed 2026-10-11, built 2026-10-12:** the dogfood profile is parked at its pinned commit with its offline tests in CI; then the workbench and `/api/session` are removed. See [Dispositions recorded](#dispositions-recorded) |
| D01-10 | Authorization | Data-path enforcement: scopes only / OpenFGA check / layered; sub-choice 10a, deploying OpenFGA (formerly D01-11) | **Disposed 2026-10-11:** layered; scopes only for the isolated trial; Check on relation-declaring resources from M3 and for every tenant beyond the trial. Designed and tested, not wired (ADR 0027). Collections: a distinct tenant-bound check per resource; the model contract (`fabric-fga-model`) and the collection Check are built. The interim refusal is gone, and a required CI job proves the model against pinned OpenFGA (§2a, amended 2026-10-12). Transport: verified TLS, plaintext only on a test loopback path (§2b) |
| D01-12 | Authorization | The administrator-role bypass and `require_scopes = false`; operator granularity | **Disposed 2026-10-11:** bypass and toggle removed; health detail role retained as the coarse operator control (ADR 0027 §5) |
| D01-13 | Authorization | Where entitlements are enforced | Applications enforce; Fabric previews. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Applications enforce entitlements server-side; Fabric previews them, without presenting a preview as enforcement. Keep the settled WorkSpec policy." |
| D01-14 | Lifecycle | What deprovisioning undoes (ADR 0021 owed 3; ADR 0008 Deletion) | **Disposed 2026-10-11:** tombstone first, retained state, confirmed deletion as a separate write; no destructive action or retention period. Built; retention and customer-data choices deferred. Restore (item 3 of 6113749198; disposition 6120741272) built 2026-10-12, under review, as source: a forward, CAS-fenced, reserved and audited write with whole-binding retained-placement checks, and the lifecycle part of O5's guard on readiness, sweeps, re-assertion and the issuer registry. **Held:** no continuity store exists, so every restore and every client's identity side effects are held; O5's other guards stay open. See [Dispositions recorded](#dispositions-recorded) |
| D01-15 | Lifecycle | `.internal` private-network clients (ADR 0021 owed 4) | Follow the host kind. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Follow the declared host kind, preserving private-network reachability. Do not expose an internal client publicly or change networking to implement this." Built 2026-10-12, under review: `.internal`-only clients project `privateNetwork` on their own hosts over a declared `http`/`https` scheme, with no public template callback; public clients unchanged; mixed host kinds refused; host moves are classified against a declaration ledger and held when no baseline is known. **Partial:** no durable ledger until the protected continuity store exists, so every move is held today. Not verified live. See ADR 0021's D01-15 amendment |
| D01-16 | Lifecycle | Client upgrades, and `automatic` for application components (ADR 0021 owed 7) | Operator save per client for the pilot. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Explicit operator save per client for the pilot; no automatic application-component upgrade policy introduced." |
| D01-18 | Lifecycle | How the runtime is raised from `replicas: 0` (amends ADR 0023 §5) | Fabric reports a gate; a person flips it. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Fabric reports the readiness gate; a deliberate operator action changes replicas only once inputs and evidence are ready. Current replicas=0 is intentional, not a crash; scaling alone cannot fix absent documents/identity/connectors. Existing disposable LucentRoot authority does not excuse unmet readiness conditions." |
| D01-19a | Lifecycle | What "client created" means (ADR 0021 owed 2) | **Disposed 2026-10-11:** explicit incomplete state until provisioning, identity and publication are met. Built. See [Dispositions recorded](#dispositions-recorded) |
| D01-19b | Lifecycle | Who owns a projected identity client (ADR 0021 owed 5) | **Disposed 2026-10-11; complete 2026-10-12** (#147, #151 merged; #153 corrected the wording): product-owned projection; unsupported hand edits refused, not silently fought. Built: identity edits to a projected client refused except when retired; product saves refuse a stored projection divergence; held Keycloak hand edits on managed fields reported as `handEdited`, overwritten only by an audited re-assertion; every client read reports a stored divergence (`projection`); sweeps withhold a diverged entry (`withheld`); re-assertion onto one is refused. ADR 0021's known limits still apply. See [Dispositions recorded](#dispositions-recorded) |
| D01-19c | Lifecycle | Activity and the size limit (ADR 0021 owed 6) | **Disposed 2026-10-11:** bounded view; durable audit source. Designed and tested, not activated (ADR 0028). Retention, storage activation and customer-data commitments deferred |
| D01-19d | Lifecycle | Accept ADR 0021 and ADR 0026 (0026 has six owed decisions; it comes after D01-2) | **Disposed 2026-10-11:** in order 0023, 0021, 0026; owed items stay owed |
| D01-20 | Rollback | Order updates as well as creation (`RollingSync`, platform #44) | Creation-only for the trial; `RollingSync` before the pilot. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Creation-only ordering may remain for the isolated disposable trial with explicit degraded-state gates; prove an ordered-update mechanism (RollingSync candidate) before the durable pilot. #44 may proceed as a bounded design/test PR. Establish controller/sync effects before any approval that would change the running environment." |
| D01-21 | Rollback | Rollback floor for Fabric's own component across format changes. #136 records and enforces a floor for the published state (`minimum_publisher_contract`) | Minimum rollable version in the release manifest. **Disposed 2026-10-11:** A. Built: `compatibility.yaml` floor `0.3.0-preview.16`, enforced for in-product rollback; publication contract 3 (2 for #139, 3 for the issuer registry, #140). Break-glass not refused; not verified live |
| D01-23 | Rollback | Owner of tenant data backup, restore and migration | Platform, gated to M5. **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Platform owns these, gated before M5/durable pilot. WorkSpec builds compatible migration artifacts; Fabric #120 owns deployment-time execution. No backup destination, credential, retention duration or real-data restore is implicitly approved." |
| D01-25 | Secrets | Lifecycle and rollback of client secret versions (ADR 0017; PR105/106). Options: A roll forward only, B version read and restore, C retention and destroy | **Disposed 2026-10-11:** A. Roll forward under check-and-set; no historical reveal, undelete or destroy. Retention documented (store default, 10 versions, measured on the fixture); limits proposed separately ([#108 comment](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6116824201)), nothing applied. See [Dispositions recorded](#dispositions-recorded) |
| D01-26 | Identity | Who configures tenant end-user sessions for client instances. ADR 0024 already sets the policy (the gateway holds the session); this is engineering ownership, and Brett confirms only if it changes D01-3(b) (#114) | A, platform per-client composition, with D01-3(b). **Disposed 2026-10-11** ([#108 comment 6109518415](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)): "Already answered: option A, platform per-client composition under Accepted ADR 0024/0025. See comment 6008710586 and the approved direction in #70 comment 6083463065. Reopen only for an actual boundary change." |
| D01-27 | Rollback | Rollback of the desired-state repositories (`saas-fabric-clients`, platform environment files). Options: A Git revert supported, B revert through the console only, C no revert of placements | C: placements roll forward only; data sources only by a revision-bumping edit; catalogue and client documents keep the Git edit as break-glass until a console restore exists. Decide with D01-1, D01-2, D01-19d. **Disposed 2026-10-11:** placements roll forward only; no blind placement revert; Git effects tested before being claimed. See [Dispositions recorded](#dispositions-recorded) |

**Retired IDs:**

| Retired | Now part of |
|---|---|
| D01-7 | D01-6b |
| D01-8 | D01-3(b) |
| D01-11 | D01-10a |
| D01-17 | D01-3(c) |
| D01-22 | D01-1 |
| D01-24 | P-1 |

## Not verified

- PR45's draft state, GitHub mergeability and CI. This session has no API
  access to `saas-fabric-platform`; the diff was read from `pull/45/head`.
- Whether a master-realm user named `brett@fieldstate.nz` exists, and what
  caused the Oct-3 `master-instance` `Degraded` (platform #48).
- Whether per-client OpenTofu routes reach the runtime's `/v1/data`.
- The state of platform #44.
- Live LucentRoot state: the publication row, the first gateway sign-in and
  the runtime replicas (Git says `0`). The Oct-3 inventory in #108 is the
  latest observation cited, and it was not re-observed.
- PR93's CI against current `main`. The last run was on the base `8676af4`.
- The contents of `saas-fabric-clients`.
