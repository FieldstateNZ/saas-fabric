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

- **ADR 0024 slice 1, Fabric side.** `GET /api/operator` is the probe for a
  gateway session. It answers `401 operator_refused` when it refuses a
  forwarded bearer (`crates/fabric-control-plane/src/handlers/get_operator.rs:15-33`).
- **ADR 0024 slice 2: not built.**
  - `GET/POST /api/session` is mounted only when `sign_in` is configured
    (`crates/fabric-control-plane/src/routes.rs:79-82`).
  - LucentRoot still configures it: platform
    `applications/core/saas-fabric-control-plane/overlays/lucentroot/control-plane.toml:54-64`
    says "`/api/session` returns this value … retires … in ADR 0024 slice 2".
  - No `/api/user/current` route exists in `crates/`.
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

### Runtime tenant identity (ADR 0019, Proposed)

**Proposed, built: the runtime half.** ADR 0019 §2 is in code:
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

**Proposed, not built: the edge half.** This is the §G checklist, G1–G17
(`docs/decisions/0019-…:1255-1283`).
- The platform runtime `HTTPRoute`
  (`applications/core/saas-fabric/base/httproute.yaml`) has no JWT policy and
  uses the host `fabric.invalid`.
- Its header comment (`:3-4`) says client hostnames such as `acme.<domain>`
  get routes "created by OpenTofu in the client's own namespace". It does not
  say whether those routes reach `/v1/data`. That conflicts with G1 ("one
  route per runtime service … no per-tenant route") only if they do.
  **Ambiguous, not verified.**
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

**U2.2: the generator for the issuer-to-tenant registry (ADR 0019 §G4a)**
(**D01-4**). Nothing generates it: see #70 and `README.md:455-457`. ADR 0023
rejected "a fourth document, now" (`docs/decisions/0023-…:387`).

| Option | Shape | For | Against |
|---|---|---|---|
| A | **The Fabric publisher generates both lists** from client realms and placements: the runtime registry inside the published tenants document (an ADR 0018 `v2` change), and the gateway's issuer allow-list | One generator, one change, as G4a requires; the realm list is already Fabric's | Schema change to a `deny_unknown_fields` document. Reverses ADR 0023's Consequence that the registry "is still configuration" (`0023-…:373-376`). A Fabric-written gateway object widens the publisher's RBAC. If the platform renders the allow-list instead, from a list Fabric publishes, that risks breaking G4a's "in the same change" (`0019-…:1268`) |
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
ADR 0024 §2 retires `/api/session`; that part is decided. ADR 0021's owed
decision 1, keep or remove the loopback workbench, is still open.

Sequencing:
- Draft PR #101 (`codex/disposable-dogfood`, head `8cd1c5f`) builds its trial
  around console sign-in through `/api/session`. Its
  `examples/disposable-dogfood/ci_smoke.py:96` defines a
  `sign_in_through_console` stage, and lines `:401-402` call `/api/session`.
- So retiring `/api/session` (ADR 0024 slice 2) and removing the workbench must
  be sequenced with PR101.
- PR102 (anonymous catalogue-write tests at the operator boundary) and PR107
  (identity role edits in the console) touch this area too. They are tracked
  in #109.

Options:
- **A.** Keep the workbench, with guard rails, and amend the control-plane
  architecture.
- **B.** Remove it, so local work needs the `hosting/` Envoy and Keycloak.

*Recommendation:* **B**, after PR101 has moved to the gateway flow or been
parked. Owner: **Brett**.

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
the per-client gateway half of U2.5 (D01-3(b)), and #114 tracks it.

Evidence:
- ADR 0024 puts one gateway filter chain per client, selected by host
  (`:50-56`), and names a `SecurityPolicy` on LucentRoot (`:80-81`).
- ADR 0025 puts the edge with the platform. Its master-instance pattern
  regenerates the gateway secret on re-run (Consequences, quoted in §5).
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
  single Envoy Gateway `SecurityPolicy` can select a realm per host, which
  ADR 0024 describes as one chain per client.

*Recommendation:* **A**, decided together with D01-3(b). Owner: engineering
ownership under ADR 0024; **Brett** confirms only if the answer changes
D01-3(b). **Not verified:** the content and state of #114, and whether ADR 0024
slices 3–5 are built.

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

- **The Data API checks scopes and roles only.** `ResourcePermissions`
  (`crates/fabric-data-api/src/authorization.rs:47-74`):
  - The scopes are `data:<resource>:read` and `data:<resource>:write`.
  - `require_scopes = false` makes `permits` return `true` for everything
    (`:75-76`).
  - `administrator_role` is configurable and defaults to `platform-admin`
    (`:62`; `examples/config.toml:189-190`). A token holding that role passes
    every check.
  - Nothing calls `fabric-fga-auth` or OpenFGA on the data path. The only
    mention is a comment in `fabric-api`
    (`src/config/validation/issuers.rs:43`).
  - Tests that information about withheld verbs does not leak:
    `crates/fabric-data-api/tests/authorization_ordering.rs::an_unauthorised_caller_cannot_tell_an_exposed_verb_from_a_withheld_one`
    (:181) and its siblings.
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
not yet enforced there; B enforced from M3 (see D01-10a). Owner: **Brett**.

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
  - **Tenant trust and isolation (M3):** **A** — OpenFGA deployed and models
    converged, so the two-tenant proof (#116) runs against the enforcement
    that tenant launch will use.
  - Any tenant served beyond the disposable trial requires A. Brett has not
    decided this schedule.

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

**U4.2: private-network (`.internal`) clients** (**D01-15**). Sources: ADR 0021
owed decision 4; #70 "Point 3".

Options:
- **A.** A projected client follows the kind of its hosts (`privateNetwork` for
  `.internal`).
- **B.** Applications are for public hosts only.

*Recommendation:* **A**, because LucentRoot needs it. Owner: **Brett**.

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
- **D01-19b: who owns a projected identity client** (ADR 0021 owed decision 5,
  `0021-…:626-628`). Evidence: §4's projection replaces entries by id
  (`0021-…:273-313`).
  - **A.** A product save keeps overwriting hand edits.
  - **B.** Projected clients are marked as the product's and refused in
    identity edits.
  - *Recommendation:* **B**.
- **D01-19c: activity and the document size limit** (ADR 0021 owed decision 6,
  `0021-…:629-633`). Evidence: today activity is bounded only by
  `422 document_too_large`.
  - **A.** Keep activity in desired state, with a trimming rule.
  - **B.** Move it to a durable audit store.
  - **C.** Drop it in favour of Git history.
  - *Recommendation:* **B**, with retention from #127.
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

**U5.3: rolling back runtime documents.** This is now part of **D01-1**
(formerly D01-22). Revisions cannot go backwards.

Options:
- **A.** Roll forward only: republish the earlier content at a new revision.
  This matches the verdict table as built.
- **B.** Allow an operator to reset the revision.
- **C.** Revert `placements.yaml` and `data-sources.yaml` in Git and let the
  publisher republish.

*Recommendation:* **A**, using C as the mechanism. Record it in ADR 0018 when
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
    mismatch is `SecretsError::Conflict` (`wire.rs:61-66`, `errors.rs:90-97`).
    The route takes `expectedVersion` (`handlers/secrets/write.rs:23-29`;
    `docs/architecture/control-plane.md:149`).
  - Delete goes to the metadata endpoint and removes **every** version,
    irreversibly (`operations.rs:69-83`; `control-plane.md:150`). There is no
    per-version delete, undelete or destroy.
  - There is no rollback path. `ClientSecrets` has list, metadata, reveal,
    write and delete only (`crates/fabric-control-plane/src/client_secrets.rs:52`).
    Reveal takes a path and no version (`handlers/secrets/reveal.rs:21-24`), and
    metadata reports only the current version and its time
    (`client_secrets/values.rs:16-22`). An older value cannot be read through
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

  *Recommendation:* **A** now, with C's retention setting made explicit in the
  platform and no destroy endpoint yet. Take **B** when an operator needs to
  read an older version. Owner: **Brett**. This is Claude's proposal, not
  something a PR or ADR settles.
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
    `src/publication/snapshot.rs:78-107`). ADR 0018 says a tenant revision "only
    ever increases" (`:407`) and the runtime ignores an older one (`:46`). A
    revert that lowers a record's revision lowers the sum, so the runtime would
    probably keep serving the old binding. **Not verified.**
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
    the console has no "restore previous" action today (**not verified**), and
    placements have no console edit beyond placing.
  - **C.** Forbid revert of `placements.yaml` and of data sources that a
    placement names. Allow a revert of the catalogue and client documents
    (through B, with Git revert as break-glass). For: protects the one part
    where a revert can strand tenant data. Against: a convention unless
    `scripts/check.py` or branch protection enforces it (**not verified**).

  *Recommendation:* **C**. Placements and data sources roll forward only
  (matching U5.3 A), and the catalogue and client documents roll back through
  B. Make the Git revert claim only after a test shows what the reconciler and
  runtime do with it. Owner: **Brett**.

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
   is not true of platform `main` until PR45 lands. Brett accepts that
   amendment and interim as part of D01-6.

---

## Decisions owed

Brett owns every policy decision; D01-26 is engineering ownership under ADR 0024 and needs Brett only if it changes D01-3(b). "Rec." is a recommendation only; none is approved. IDs merged during
review are noted, and the original IDs are not reused.

| ID | Area | Decision | Rec. |
|---|---|---|---|
| D01-1 | Publication | Accept or amend ADR 0018 (built, Proposed), including runtime-document rollback (U5.3, formerly D01-22) | Amend "as built", accept; roll forward only |
| D01-2 | Publication | Accept or amend ADR 0023: resolve the non-shared placement contradiction (`:222-228`/`:370-372` vs `:230-235`, implemented by `select.rs:44-47`); §5 against D01-18; fix the stale `README.md:451-454` | Amend, then accept |
| D01-3 | Provisioning | Per-client platform composition, three sub-choices: (a) databases, schemas and connector processes; (b) client instance resources (formerly D01-8); (c) application component deployment (formerly D01-17) | Platform composition per client for all three |
| D01-4 | Identity | Generator for the issuer-to-tenant registry (ADR 0019 §G4a) | Fabric publisher, one generator; gateway sub-choice open |
| D01-5 | Identity | Accept ADR 0019; the platform owns the §G edge; clarify the client-route comment | Accept as written |
| D01-6 | Identity | How operator master-realm accounts come to exist (broker / create / `admin` only / `[]`); sub-choice 6b, revocation (formerly D01-7); accepting PR93's interim | `admin` interim, brokering as target; Brett names the upstream |
| D01-9 | Session | Keep or remove the loopback workbench as `/api/session` retires (ADR 0021 owed 1); sequence with PR101 | Remove, after PR101 is settled |
| D01-10 | Authorization | Data-path enforcement: scopes only / OpenFGA check / layered; sub-choice 10a, deploying OpenFGA (formerly D01-11) | Layered; scopes only for the isolated M1 trial; OpenFGA enforced from M3 and for any tenant beyond the trial |
| D01-12 | Authorization | The administrator-role bypass and `require_scopes = false`; operator granularity | Remove the bypass, forbid disabling scopes outside tests; operators stay coarse |
| D01-13 | Authorization | Where entitlements are enforced | Applications enforce; Fabric previews |
| D01-14 | Lifecycle | What deprovisioning undoes (ADR 0021 owed 3; ADR 0008 Deletion) | Tombstone, retention, confirmed delete |
| D01-15 | Lifecycle | `.internal` private-network clients (ADR 0021 owed 4) | Follow the host kind |
| D01-16 | Lifecycle | Client upgrades, and `automatic` for application components (ADR 0021 owed 7) | Operator save per client for the pilot |
| D01-18 | Lifecycle | How the runtime is raised from `replicas: 0` (amends ADR 0023 §5) | Fabric reports a gate; a person flips it |
| D01-19a | Lifecycle | What "client created" means (ADR 0021 owed 2) | Explicit "incomplete" state |
| D01-19b | Lifecycle | Who owns a projected identity client (ADR 0021 owed 5) | Owned by the product; hand edits refused |
| D01-19c | Lifecycle | Activity and the size limit (ADR 0021 owed 6) | Durable audit store |
| D01-19d | Lifecycle | Accept ADR 0021 and ADR 0026 (0026 has six owed decisions; it comes after D01-2) | In order: 0023, 0021, 0026 |
| D01-20 | Rollback | Order updates as well as creation (`RollingSync`, platform #44) | Creation-only for the trial; `RollingSync` before the pilot |
| D01-21 | Rollback | Rollback floor for Fabric's own component across format changes | Minimum rollable version in the release manifest |
| D01-23 | Rollback | Owner of tenant data backup, restore and migration | Platform, gated to M5 |
| D01-25 | Secrets | Lifecycle and rollback of client secret versions (ADR 0017; PR105/106). Options: A roll forward only, B version read and restore, C retention and destroy | A now, with explicit retention; B when an older version must be read |
| D01-26 | Identity | Who configures tenant end-user sessions for client instances. ADR 0024 already sets the policy (the gateway holds the session); this is engineering ownership, and Brett confirms only if it changes D01-3(b) (#114) | A, platform per-client composition, with D01-3(b) |
| D01-27 | Rollback | Rollback of the desired-state repositories (`saas-fabric-clients`, platform environment files). Options: A Git revert supported, B revert through the console only, C no revert of placements | C: placements and data sources roll forward only; catalogue and client documents through the console |

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
