# M0 — Fabric lifecycle and authorization contract decisions

Deliverable for [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108)
(W0, gate M0, planning key `D01`). Read-only analysis: no code, no provider
apply, no credential change, nothing merged.

| Source | Ref | Head read |
|---|---|---|
| `FieldstateNZ/saas-fabric` | `main` | `1f46788ecd5c59af1a4937aae0e51cf59b8f8081` |
| `FieldstateNZ/saas-fabric` PR #93 | `claude/adr-0025-gate-and-accounts` | `647ef8dd808fc59184e467991c5dee681a90d16b` (base `8676af4`) |
| `FieldstateNZ/saas-fabric-platform` | `main` | `0ff5d669d36a6c5c0b133bf4e6ced74764504f11` |
| `FieldstateNZ/saas-fabric-platform` PR #45 | `pull/45/head` | `fd12327373820b545ddcf33bd84887a4c3c18a50` (parent `355fe6d`) |
| Date | | 2026-10-05 |

**Reading rules.** "Decided" means an ADR with status Accepted. A Proposed ADR
is reported as *Proposed — built* or *Proposed — not built*, never as decided,
even where code implements it. Cluster facts are not re-observed here; anything
this document could not check is marked **not verified**. Every unresolved
choice has decision owner **Brett** (product owner). Recommendations are
labelled as such and are not decisions.

ADR status at `1f46788`: Accepted — 0001–0003, 0005–0017 (0014 for the control
plane only), 0020, 0022 (LucentRoot readiness milestone only), 0024, 0025.
Superseded — 0004. **Proposed — 0018, 0019, 0021, 0023, 0026.**

---

## 1. Provisioner and publication ownership

### Decided

- **Desired state is the authority** — ADR 0008 (Accepted). Nothing is
  inferred from a label — ADR 0007 (Accepted); a shared data source serves
  discriminator isolation only — ADR 0006 (Accepted).
- **Platform repository owns wiring, not data** for the runtime documents: the
  platform declares the Role, RoleBinding and whole-volume mounts; the three
  `fabric-runtime-*` ConfigMaps are declared nowhere in Git. This is stated in
  ADR 0018 and ADR 0023, **both Proposed**, so it is *not decided* — but it is
  built on both sides (platform #40, per #70 comment of 2026-09-20; platform
  `applications/core/saas-fabric/base/configmap.yaml:25-39`).

### Built under Proposed ADRs (evidence, not decision)

| Contract | ADR (status) | Code | Tests |
|---|---|---|---|
| Three versioned documents, monotonic revision, byte-compared divergence, refuse emptying/dangling refs | 0018 (Proposed) | `crates/fabric-runtime-publication/src/validate.rs:59-136`, `src/verdict.rs` | `verdict_tests.rs::an_older_revision_against_a_held_manifest_and_payload_is_refused` (:66), `::the_same_revision_with_different_bytes_is_refused_as_divergent` (:81); `tests/published_state_serves_two_tenants.rs::a_stale_revision_publication_is_refused_and_the_last_good_files_remain` (:262), `::an_emptying_publication_is_refused_unless_it_is_intended` (:463); mutation table `docs/verification.md:228-305` |
| Publisher is a control-plane controller with its own ServiceAccount, on a schedule and on `POST /api/platform/publication` | 0023 §4 (Proposed) | `fabric-platform-management/src/publication/*`; route doc `crates/fabric-control-plane/src/routes.rs` (the `/api/platform/publication` line) | `crates/fabric-control-plane/tests/platform_publication.rs::publishing_composes_declared_state_into_the_runtimes_three_documents_and_settles_unchanged` (:200), `::a_catalogue_with_no_published_resources_is_waiting_and_writes_nothing` (:353) |
| Kubernetes adapter over plain HTTPS, `get/create/update`, no `delete`, no `kube` crate | 0023 §4 amending 0018 (Proposed) | `crates/fabric-publication-kubernetes/src/publish.rs` | `publish_tests.rs::a_first_publication_creates_three_objects_in_order_with_the_label` (:24), `::a_stale_snapshot_is_refused_before_any_write` (:178), `::a_document_past_the_object_cap_is_refused_before_any_write` (:207) |
| Data sources are environment desired state (`data-sources.yaml`); placement is a recorded Fabric write (`placements.yaml`) | 0023 §1–2 (Proposed) | `crates/fabric-platform-management/src/placements/select.rs:26-56` | `placements/select_tests.rs::rule_3_a_shared_candidate_isolates_by_the_tenant_id_as_the_discriminator_value` (:257); `crates/fabric-control-plane/tests/platform_placements.rs::a_clients_data_intent_is_placed_recorded_and_protects_its_data_source` (:180) |

Deployed evidence: #70 (2026-09-22) records platform #41 merged as `dc5323a`
enabling `[platform_management.publication]` on LucentRoot, with expected row
`waiting`. **Not verified** here whether the row now reads `waiting`,
`published`, `refused` or `failed`.

### Unresolved

**U1.1 — Who provisions a tenant's database, schema and connector process.**
ADR 0018 names `ProvisionedPlacement` as an input with "no owner yet"
(`docs/decisions/0018-…:512-579`); ADR 0023 refuses provisioning and the
connector deployment (`docs/decisions/0023-…:389-405`). Brett's direction
(#70, 2026-09-22): *a connector definition is a platform contract; provisioning
is per client — ADR to follow*. No ADR exists at `1f46788`. ADR 0026 also
leaves "connector kinds" undecided (`docs/decisions/0026-…:890-907`).
Note: `select.rs:44-47` already places a tenant on a *pre-declared*
non-shared data source with no prior placement; ADR 0023 Consequences says
"placement on anything but a shared source is refused until provisioning
exists". The code and the Proposed text disagree on wording — **D01-2**.

| Option | Shape | For | Against |
|---|---|---|---|
| A | **Platform composition per client**: OpenTofu Application per client (the `master-instance` pattern, ADR 0025) creates the database/role, the connector Deployment and its `[[connectors]]` entry; Fabric declares the data source and records placement | Same mechanism already proven for the master realm; keeps ADR 0012's "no standing credential in the product"; Git explains the environment | A per-client platform change for every new client; needs a trigger from Fabric's client creation to the platform repo |
| B | **Fabric controller provisions** with its own database-admin credential | One write path; client creation can complete end to end | Reverses ADR 0012's posture for a new provider; a standing high-privilege credential in the control plane |
| C | **Pre-provisioned pools only** (status quo): operators declare shared data sources; dedicated is manual | Nothing new to build | Dedicated tenants never self-serve; connector deployment still has no owner |

*Recommendation:* **A**, consistent with Brett's 2026-09-22 direction and ADR
0025's boundary. Decision owner: **Brett** — **D01-3**.

**U1.2 — Whether ADR 0018 and ADR 0023 are accepted as built.** Both are
implemented, wired on LucentRoot, and still Proposed; ADR 0026 states that
accepting it accepts the parts of 0021/0023 it builds on
(`docs/decisions/0026-…:18-20`). Options: A accept both with their
"as built" amendments; B amend first (e.g. the dedicated-placement wording
above, the README line at `README.md:451-454` that still says "the console
does not yet show that row", which #81 built); C leave Proposed.
*Recommendation:* **B then accept** — small text fixes, then Accepted.
Owner: **Brett** — **D01-1** (0018), **D01-2** (0023).

---

## 2. Identity route and session ownership

### Decided

- **Trusted ingress is the canonical runtime posture** — ADR 0002 (Accepted);
  platform runtime config `mode = "trusted_ingress"`
  (`saas-fabric-platform: applications/core/saas-fabric/base/configmap.yaml:23-24`).
- **Operator identity is not tenant identity** — ADR 0009; operators sign in
  against the platform (master) realm — ADR 0010 (both Accepted).
- **Sign-in belongs to the gateway; the browser never holds a token; one
  route `GET /api/user/current`; surfaces are federated modules** — ADR 0024
  (Accepted 2026-09-22).
- **Master-realm instance resources (gateway client, `fabric-operator` role,
  operator grants) are platform composition**; ADR 0012 still governs client
  realms — ADR 0025 (Accepted 2026-09-22).

Evidence of what is built:

- ADR 0024 slice 1 (fabric side): `GET /api/operator` is the gateway-session
  probe and answers `401 operator_refused` for a refused forwarded bearer —
  `crates/fabric-control-plane/src/handlers/get_operator.rs:15-33`.
- ADR 0024 slice 2 **not built**: `GET/POST /api/session` still mounted
  (`crates/fabric-control-plane/src/routes.rs:36`, `:79-82`);
  no `/api/user/current` route exists in `crates/`.
- ADR 0024 slices 3–5 **not built**: `apps/app-shell` has no sign-in and no
  runtime module loading; no `/v1/identity` route on the runtime.
- Platform side: OIDC `SecurityPolicy` at
  `applications/core/saas-fabric-control-plane/overlays/lucentroot/oidc.yaml`
  and the `master-instance` convergence (platform #43, `355fe6d`). First
  sign-in through the gateway: **not verified**.
- **Platform `main` roster is broken.** `master-instance-config.yaml:40` on
  `0ff5d66` still reads `operators: '["brett@fieldstate.nz"]'`, an account the
  master realm does not hold; PR45's README records three plan-time failures
  and `Degraded` on 2026-09-22. #108's Oct-3 inventory also reports degraded
  master-instance convergence. PR45 changes the roster to `["admin"]`.

### Runtime tenant identity (ADR 0019, Proposed)

- *Proposed — built (runtime half):* §2 issuer names the tenant, claim must
  agree, empty registry refuses start —
  `crates/fabric-identity/src/resolver/tenant_binding.rs:45-57`,
  `src/identity/trusted_issuer.rs:101-131`; tests
  `crates/fabric-identity/src/resolver_tests.rs::a_token_from_an_unregistered_issuer_is_refused` (:126),
  `::a_token_with_no_issuer_is_refused_rather_than_treated_as_unregistered` (:139),
  `::a_tenant_claim_that_disagrees_with_its_issuer_is_refused` (:165).
- *Proposed — not built (edge half):* §G checklist G1–G17
  (`docs/decisions/0019-…:1255-1283`). Platform runtime `HTTPRoute`
  (`applications/core/saas-fabric/base/httproute.yaml`) has no JWT policy and
  host `fabric.invalid`; its header comment says client routes "are created by
  OpenTofu in the client's own namespace", which contradicts G1's "one route
  per runtime service, no per-tenant route". Runtime config has no
  `[identity]` section, so the runtime would refuse to start even with
  documents published (`trusted_issuer.rs:111`). Runtime held at
  `replicas: 0` (`applications/core/saas-fabric/base/deployment.yaml:20`).

### Unresolved

**U2.1 — Accept ADR 0019, and who owns the edge (§G).** Options: A accept
as written, platform implements G1–G17; B amend G1 to the per-client-route
shape the platform comment describes; C defer the edge and keep the runtime
at zero. *Recommendation:* **A** — the runtime half is built and tested to
the single-route model; the platform comment is the stale party. Owner:
**Brett** — **D01-5**.

**U2.2 — The issuer-to-tenant registry generator (ADR 0019 §G4a).** Nothing
generates it (#70; `README.md:455-457`; ADR 0023 rejected "a fourth document,
now", `docs/decisions/0023-…:387`).

| Option | Shape | For | Against |
|---|---|---|---|
| A | **Fabric publisher generates both** from client realms + placements: the runtime registry inside the published tenants document (an ADR 0018 `v2` change), and the gateway's issuer allow-list as a Fabric-written object | One generator, one change (G4a's own requirement); the realm list is already Fabric's | Schema change to a `deny_unknown_fields` document; widens the publisher's RBAC to a gateway policy object |
| B | **Platform composition** generates both per client (the U1.1-A Application) | Matches ADR 0025's edge-is-platform boundary | Two repositories must agree on the tenant list; Fabric's placement is the source of truth for which tenants exist |
| C | Hand-maintained `[identity].trusted_issuers` + gateway list | Fastest for one trial tenant | Exactly the drift G4a forbids |

*Recommendation:* **A** for the runtime registry; whether the gateway
allow-list is Fabric-written or platform-rendered from a Fabric-published
list is the sub-choice. Owner: **Brett** — **D01-4**.

**U2.3 — How an operator's master-realm account comes to exist.** The
convergence grants roles and creates no accounts (PR93 / PR45). Options:
A **broker to an upstream IdP** (Karo's model; Brett names the upstream);
B convergence **creates declared local accounts**; C bootstrap `admin`
remains the only operator. *Recommendation:* merge PR45/PR93 now so the
roster is true (C as interim), then **A**. Owner: **Brett** — **D01-6**.

**U2.4 — Operator revocation.** Grants are `exhaustive = false`; removing a
name revokes nothing (PR45 README "Who the operators are"; #70
2026-09-22). Options: A exhaustive grant management in the convergence;
B revocation stays a Keycloak act, documented; C revoke at the upstream IdP
once U2.3-A exists. *Recommendation:* **C** if U2.3 is A, else **A**.
Owner: **Brett** — **D01-7**.

**U2.5 — Who provisions a client *instance* (its realm's gateway client,
host, OIDC policy).** ADR 0025 leaves it open, naming Karo's "every directory
alike" as the expected answer (`docs/decisions/0025-…:133-140`); client realms
themselves stay with the control plane under ADR 0012. Options: A platform
composition per client instance, Fabric continues reconciling realm
*content*; B the control plane creates the instance resources as the
operator (ADR 0012 posture); C every client shares the master pattern
unchanged. *Recommendation:* **A**, decided together with D01-3 since both
are a per-client platform Application. Owner: **Brett** — **D01-8**.

**U2.6 — The loopback workbench and `/api/session` retirement.** ADR 0024 §2
retires `/api/session` (decided); ADR 0021 owed decision 1 (keep/remove the
loopback workbench) is still open and affects how the console is developed
once its own sign-in is gone. Options: A keep with guard rails, amend
control-plane architecture; B remove, local work needs `hosting/` Envoy +
Keycloak. *Recommendation:* **B**, since ADR 0024 already puts the gateway on
the local path. Owner: **Brett** — **D01-9**.

---

## 3. Authorization enforcement

### Decided

- **Authorization is declared in the platform's words** (resources →
  relations → operations) and memberships are runtime data, not desired
  state — ADR 0013 (Accepted); model in `crates/fabric-client-model/src/authorization.rs`.
- **Fabric owns the OpenFGA front door**: identity bound from the verified
  `iss`; two listeners (runtime surface, control-plane surface) — ADR 0016
  (Accepted). Runtime surface built in `crates/fabric-fga-auth/` (registry
  pins `authorization_model_id`, `src/registry.rs:129-131`).
- **Control plane calls OpenFGA as the operator** — ADR 0014 (Accepted, control
  plane only).
- **Fabric decides which client secret boundary an operation reaches** —
  ADR 0017 (Accepted).
- **Operator authority is coarse and separate from tenants** — ADR 0009;
  `docs/verification.md` "What is not verified": "Every authenticated operator
  may do everything the API offers."

### What actually enforces today

- **Data API:** scope/role check only. `ResourcePermissions`
  (`crates/fabric-data-api/src/authorization.rs:47-74`): scopes
  `data:<resource>:read|write`, or the `platform-admin` role (`:62`) bypasses.
  No call to `fabric-fga-auth` or OpenFGA on the data path (no reference in
  `fabric-data-api`, `fabric-api`, `fabric-tenant-runtime`). Ordering tests:
  `crates/fabric-data-api/tests/authorization_ordering.rs::an_unauthorised_caller_cannot_tell_an_exposed_verb_from_a_withheld_one` (:181) and siblings.
- **OpenFGA is not deployed**: platform
  `applications/core/openfga/README.md:11` — "**not deployed** — no
  `application.yaml`". ADR 0016's control-plane surface and the converging of
  `spec.authorization` into a store/model are not built
  (`docs/architecture/control-plane.md:1607-1620`: "OpenFGA ← not built").
- **Entitlements:** ADR 0021 (Proposed) — "Navigation is an entitlement
  preview; the application enforces its own permissions"
  (`docs/decisions/0021-…:591-592`). ADR 0024 §3 (Accepted) — `roles` is the
  realm tier, finer checks belong to the module.

### Unresolved

**U3.1 — What enforces data-path authorization for a served tenant.**

| Option | Shape | For | Against |
|---|---|---|---|
| A | **Scopes/roles only** (status quo) for the first tenant; ReBAC later | Built and tested; no new service on the critical path | `spec.authorization` declares relations nothing enforces; `platform-admin` is a tenant-realm role bypass |
| B | **Data API calls `fabric-fga-auth` `Check`** after scope check | Enforces what ADR 0013 lets clients declare | Requires OpenFGA deployed, model convergence, and tuple-writing API; new failure mode (503) on every request |
| C | Layered: A now, B per resource opt-in once OpenFGA is deployed | Incremental | Two models to explain to operators |

*Recommendation:* **C**, with A as the M3 trial posture and an explicit
statement that declared relations are not yet enforced. Owner: **Brett** —
**D01-10**.

**U3.2 — Who deploys OpenFGA and converges `spec.authorization` into it.**
Options: A platform deploys the `fabric-openfga` image, Fabric reconciles
models via ADR 0016's control-plane surface; B defer OpenFGA until after the
pilot. *Recommendation:* follows D01-10; if C, then **B** for M3 and **A**
before M4. Owner: **Brett** — **D01-11**.

**U3.3 — The `platform-admin` bypass and operator granularity.** Options:
A keep a realm-role bypass in the Data API; B remove it (administrators
use scopes like everyone); C operator roles finer than `fabric-operator`.
*Recommendation:* **B** for the Data API (a tenant-realm role should not be an
all-resource bypass by default); operator granularity stays coarse until a
second operator persona exists. Owner: **Brett** — **D01-12**.

**U3.4 — Entitlement enforcement point.** Whether Fabric enforces plan
entitlements (e.g. a `/api/user/configuration` per ADR 0024's Karo reference)
or only previews them. Options: A applications enforce, Fabric previews
(ADR 0021 text); B Fabric enforces at the edge/runtime by plan. *Recommendation:*
**A** for M4, revisit with commercial policy (#127). Owner: **Brett** —
**D01-13**.

---

## 4. Service lifecycle

### Decided

- **Fabric's own components** advance automatically, can be paused (hold) and
  rolled back with a hold; desired state in platform Git — control-plane
  architecture `docs/architecture/control-plane.md:698-720`; running version
  comes from deployment evidence — ADR 0022 (Accepted, milestone-scoped);
  tests `crates/fabric-deployment-kubernetes/src/evaluate_tests.rs::stopped_workloads_require_completed_scale_down` and others listed at `docs/verification.md:517-592`.
- **Master realm lifecycle needs no human** — ADR 0025 §5 (Accepted).
- **Nothing in identity reconciliation deletes** — ADR 0008 "Deletion"
  (Accepted).

### Proposed (built, not decided)

- Console creates clients; assignments projected as public identity clients;
  **removing an assigned application is refused** — ADR 0021 §2, §4, §5
  (Proposed). `crates/fabric-control-plane/src/service/set_product.rs:45-57`;
  test `crates/fabric-control-plane/tests/product_workflows.rs::put_product_refuses_to_remove_an_assigned_application_and_leaves_the_client_unchanged` (:330).
- Data source removal refused while a tenant is placed (route list,
  `routes.rs`); runtime fails closed when a data source disappears —
  `crates/fabric-data-api/tests/data_source_lifecycle.rs::removing_a_data_source_makes_its_tenants_fail_closed` (:156).
- **Runtime raise is a manual one-line platform change** once publication is
  complete, connectors are listed and the issuer registry covers placed
  tenants — ADR 0023 §5 (Proposed), `docs/decisions/0023-…:316-325`.
- Components describe themselves (OCI descriptor); selection is an operator
  act — ADR 0026 (Proposed).

### Not built

No controller deploys application components, issues DNS/certificates or
observes their health (`README.md:468-470`); no client deletion
(`README.md:463-467`); no connector deployment (§1).

### Unresolved

**U4.1 — Deprovisioning semantics** (ADR 0021 owed 3; ADR 0023 "Deprovisioning").
Options: A tombstone — mark removed, revoke identity client, keep data for a
retention window, then delete with confirmation; B hard delete with typed
confirmation; C keep refusing removal (status quo, exit is a Git hand edit).
*Recommendation:* **A**, with retention set by #127. Owner: **Brett** —
**D01-14**.

**U4.2 — Private-network (`.internal`) clients** (ADR 0021 owed 4; #70
"Point 3"). Options: A projected client follows the host kind
(`privateNetwork` for `.internal`); B applications are public-host only.
*Recommendation:* **A** — LucentRoot needs it. Owner: **Brett** — **D01-15**.

**U4.3 — Client upgrades and `automatic` for application components**
(ADR 0021 owed 7; ADR 0026 "What this does not decide"). Options: A every
client move is an operator save; B per-client policy mirroring Fabric's own
`automatic`/hold; C catalogue-level rollout waves. *Recommendation:* **A** for
the pilot. Owner: **Brett** — **D01-16**.

**U4.4 — Who deploys an application's components** (ADR 0021 / ADR 0026 both
leave it open). Options: A platform composition per client (with D01-3/D01-8);
B a Fabric deployment controller with namespaced write RBAC. *Recommendation:*
**A**, one per-client platform Application covering D01-3, D01-8 and this.
Owner: **Brett** — **D01-17**.

**U4.5 — Raising the runtime.** Options: A manual one-line change after a
checklist (ADR 0023 §5 as Proposed); B Fabric reports a "ready to raise" gate
and a person flips it; C automate. *Recommendation:* **B**. Owner: **Brett** —
**D01-18**.

**U4.6 — Remaining ADR 0021 owed items** 2 (what "client created" means
before routing/data/secret boundary), 5 (who owns a projected identity
client), 6 (activity size/retention), and acceptance of ADR 0021 and ADR 0026
(with its six owed items, `docs/decisions/0026-…:909-935`). Not analysed in
depth here; listed so they are not lost. Owner: **Brett** — **D01-19**.

---

## 5. Rollback boundaries

### Decided / documented

| Boundary | Rule | Source |
|---|---|---|
| Fabric's own component (images) | Roll back to an older published version; restores version **and** bytes; writes a hold (`reason: rollback`) in the same commit; a digest in the request is refused | `docs/architecture/control-plane.md:722-780, 960-1030`; `crates/fabric-control-plane/tests/platform_integration.rs::a_rollback_carrying_a_digest_is_refused_rather_than_ignored` (:200); `platform_described.rs::a_described_rollback_candidate_names_its_commit_and_no_digest` (:137) |
| Fabric's own component (chart) | Restores version, not provably bytes; stated, not enforced | same, `:744-760` |
| Config across an observer boundary | Older binaries reject new config; rollback must remove config with image pins | ADR 0022 "Rollout" (Accepted) |
| Runtime documents | Revision is monotonic; an older revision is refused; deprovisioning is an empty set inside a document, never a delete; breaking change ships as new file names beside old | ADR 0018 §6, §9 (Proposed); tests in §1 |
| Master instance | Idempotent apply + drift check; state loss recovered by adoption; `prevent_destroy`; gateway secret rotation is regenerate + re-run | ADR 0025 Consequences (Accepted) |
| Wave ordering | Holds at environment **creation** only; on an existing environment children sync independently | PR93 / PR45 (open, not merged); observed 2026-09-22 per PR45 text — **not re-verified** |
| Identity reconciliation | Adds and corrects, never deletes | ADR 0008 (Accepted) |

### Unresolved

**U5.1 — Ordering updates, not only creation** (platform #44, `RollingSync`).
Options: A adopt `RollingSync`; B accept creation-only ordering and rely on
loud `Degraded` + manual revert of the OIDC policy; C a pre-sync health check
in wave 40. *Recommendation:* **B** for the disposable trial, **A** before the
durable pilot. Owner: **Brett** — **D01-20**. Platform #44 state: **not
verified**.

**U5.2 — Rollback floor for Fabric's own component across a format change**
(ADR 0026 "What this does not decide"). Options: A record a minimum rollable
version per release and refuse below it; B document only; C forward-fix only.
*Recommendation:* **A**, as part of the release compatibility manifest (#124).
Owner: **Brett** — **D01-21**.

**U5.3 — Rolling back runtime documents.** Revisions cannot go backwards.
Options: A roll-forward only — republish prior content at a new revision
(matches the built verdict table); B allow an operator revision reset;
C rely on Git revert of `placements.yaml`/`data-sources.yaml` and let the
publisher republish. *Recommendation:* **A**, with C as the mechanism; record
it in ADR 0018 on acceptance. Owner: **Brett** — **D01-22**.

**U5.4 — Tenant data rollback and schema migration.** Nothing creates tables
(ADR 0023) and off-host recovery is unproven (#108 evidence limits; platform
#50). Options: A platform owns backup/restore per data source; B Fabric
records restore points beside placements; C out of scope until the durable
pilot. *Recommendation:* **A**, gated to M5. Owner: **Brett** — **D01-23**.

---

## PR state revalidated

| | saas-fabric PR #93 | saas-fabric-platform PR #45 |
|---|---|---|
| Head | `647ef8d` (1 commit, 1 file, +20/−3) | `fd12327` (1 commit on `355fe6d`, 5 files, +110/−34) |
| Draft / ready | Ready (not draft), open | **Not verified** — GitHub API access to this repository is not enabled for this session |
| Mergeability | API `mergeable_state: clean`; local `git merge-tree` against `1f46788` clean; `main` has moved 7 commits (#94–#100) since base `8676af4`, none touching ADR 0025 | Local `git merge-tree` against `0ff5d66` clean; `main` moved 2 commits (#46, #47) since `355fe6d`; #46 touched `saas-fabric-control-plane/README.md`, which PR45 also edits, without textual conflict. GitHub mergeability **not verified** |
| CI | 24/24 check runs `success` on `647ef8d` (2026-09-22: workspace tests, clippy, fmt, deny, doc, architecture invariants, file-size, control-plane-ui, ndc acceptance, CodeQL). Not re-run against current `main` | **Not verified** |
| What it decides | Docs only. ADR 0025 wave-gate sentence corrected (gate holds at creation, not every update; #44 tracks updates); adds "what this does not decide": operator *account* origin, Karo brokering as reference, upstream is Brett's | Roster `["brett@fieldstate.nz"]` → `["admin"]` (functional: fixes the plan-time lookup failure); READMEs state when the wave gate holds; records brokering as the open question |
| Recommended disposition | **Merge** after Brett confirms D01-6 interim (bootstrap admin as operator) — a text correction to an Accepted ADR, no contract change | **Merge first** of the two after CI is confirmed on `fd12327`: platform `main` still names an account that does not exist, so `master-instance` stays `Degraded` (matches #108's Oct-3 inventory). Merging triggers a LucentRoot sync — that is an operational act needing separate authorisation |

---

## Decisions owed

All owned by **Brett**. "Rec." is a recommendation only.

| ID | Area | Decision | Rec. |
|---|---|---|---|
| D01-1 | Publication | Accept or amend ADR 0018 (built, Proposed) | Amend "as built", accept |
| D01-2 | Publication | Accept or amend ADR 0023; reconcile dedicated-placement wording with `select.rs:44-47`; fix stale `README.md:451-454` | Amend, accept |
| D01-3 | Provisioner | Who provisions tenant databases/schemas and connector processes (Brett's 2026-09-22 direction, ADR owed) | Platform composition per client |
| D01-4 | Identity | Generator for the issuer-to-tenant registry (ADR 0019 §G4a) | Fabric publisher, one generator |
| D01-5 | Identity | Accept ADR 0019; platform owns §G edge; correct per-client-route comment | Accept as written |
| D01-6 | Identity | How operator master-realm accounts exist (broker upstream / create / admin only) | Merge PR45+PR93 (admin interim), then broker; Brett names upstream |
| D01-7 | Identity | Operator revocation mechanism | Revoke at upstream IdP (or exhaustive grants) |
| D01-8 | Identity | Who provisions client instances (gateway client, host, OIDC policy) | Platform composition, with D01-3 |
| D01-9 | Session | Keep or remove loopback workbench as `/api/session` retires (ADR 0021 owed 1) | Remove |
| D01-10 | Authorization | Data-path enforcement: scopes only / FGA check / layered | Layered; scopes-only for M3 trial |
| D01-11 | Authorization | Who deploys OpenFGA and converges `spec.authorization` | Defer to before M4 |
| D01-12 | Authorization | `platform-admin` bypass in Data API; operator granularity | Remove bypass; operators stay coarse |
| D01-13 | Authorization | Entitlement enforcement point | Applications enforce; Fabric previews |
| D01-14 | Lifecycle | Deprovisioning semantics (ADR 0021 owed 3) | Tombstone + retention + confirmed delete |
| D01-15 | Lifecycle | `.internal` private-network clients (ADR 0021 owed 4) | Follow host kind |
| D01-16 | Lifecycle | Client upgrades / `automatic` for application components (ADR 0021 owed 7) | Operator save per client for pilot |
| D01-17 | Lifecycle | Who deploys application components | Platform composition per client |
| D01-18 | Lifecycle | How the runtime is raised from `replicas: 0` | Fabric-reported gate, person flips |
| D01-19 | Lifecycle | ADR 0021 owed 2, 5, 6; accept ADR 0021 and ADR 0026 (+ its six owed) | Separate session |
| D01-20 | Rollback | Order updates as well as creation (`RollingSync`, platform #44) | Creation-only for trial; RollingSync before pilot |
| D01-21 | Rollback | Rollback floor for Fabric's own component across format changes | Min rollable version in release manifest |
| D01-22 | Rollback | Runtime document rollback | Roll-forward only |
| D01-23 | Rollback | Tenant data backup/restore and migration owner | Platform, gated to M5 |
| D01-24 | PRs | Dispositions of PR93 and platform PR45 | Merge both, PR45 first, after CI and authorisation |

## Not verified

- Platform PR45 draft state, GitHub mergeability and CI (no API access to
  `saas-fabric-platform` in this session; diff read from `pull/45/head`).
- Platform issue #44 state.
- Live LucentRoot state: publication row, `master-instance` health, first
  gateway sign-in, runtime replicas (Git says `0`). The Oct-3 inventory in
  #108 is the latest observation cited, not re-observed.
- PR93 CI against current `main` (last run on base `8676af4`).
- `saas-fabric-clients` contents.
