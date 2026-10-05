# M0 — WorkSpec capability map and issue reconciliation

Deliverable for FieldstateNZ/saas-fabric#110 `[W0] Reconcile the whole WorkSpec capability map with existing issues` (planning key D03, gate M0).

| | |
|---|---|
| Date | 2026-10-05 |
| WorkSpec source head | `47607db` (FieldstateNZ/workspec `main`, commit "feat(decide): pin moment modal + revisit conditions (#507)", committed 2026-07-05). Shallow clone: no history, so "when did this change" cannot be answered from this clone. |
| Fabric source head | `1f46788` (FieldstateNZ/saas-fabric `main`). ADR 0024 (Accepted), ADR 0021 and ADR 0026 (both Proposed), `docs/` read. |
| Issue state read | GitHub MCP, 2026-10-05. WorkSpec issue list: 47 open. Fabric #110 and #118-#122 read in full. |
| Status | Analysis and documentation only. No code changed, nothing committed, no issues created or commented. |
| Product premise | WorkSpec is the complete product (Studio, Enterprise agents/MCP/mobile, cost, administration), not Studio-only and not an MVP. WorkSpec `CLAUDE.md` forbids MVP/thin-slice framing and backwards-compat shims. |
| Approval status | Every "Recommended disposition" below is a recommendation and is **pending Brett approval**. Nothing here is an approved launch disposition. |

## How this was checked, and its limits

- Read first: WorkSpec `CLAUDE.md` (there is no `AGENTS.md`; `.claude/` holds only `launch.json` and one plan file). Canonical rules applied: API surface owned by `lib/api-spec/openapi.yaml`; thin routes + per-domain services; migrations hand-registered in `_journal.json`; real-Postgres Testcontainers service tests, no DB mocks; Zod v4; no shims; no MVP framing.
- Issue states are revalidated against source at `47607db` by reading code, migrations, workflows and test files. Issue titles and bodies were not accepted as proof either way.
- **No tests were executed.** The suite needs Docker/Testcontainers and the workspace has no installed `node_modules`. "Test evidence" therefore means "a test file exists at this head that targets the behaviour", not "it passes". Pass status needs CI at the exact head.
- Verdict vocabulary: **still reproduces** (defect visible in source at head), **fixed on main** (source at head contradicts the issue; issue is stale-open), **partially mitigated** (some acceptance items met, named items not), **not built** (feature issue, code not present), **not verified** (read but not checked deeply enough to assert).
- Cluster/deployment facts from #110's evidence-limits paragraph are historical observations and are not used as proof of anything here.
- No capability named "plans/comparison" exists in WorkSpec by that name. See row 3: the matrix maps it to what the source actually contains and flags that Brett must confirm the intended meaning.

## Fabric-side context that shapes every integration gap

From ADR 0024 / 0021 / 0026 and `docs/architecture/`:

- An instance is a Keycloak realm resolved from the **host**; sign-in belongs to the **gateway** (Envoy OIDC, gateway cookies); the browser never holds a token; upstreams get `Authorization: Bearer`. Surfaces are **Module Federation 2.0** modules exposing `./Module` taking `{ basePath, user }`, routed under `#/<module id>/…`, served same-origin under `/modules/<id>/`.
- The catalogue (ADR 0021, Proposed) models an application with an id (= identity client id), releases, navigation entries each carrying a permission name the application enforces, and per-client plan/configuration.
- A component describes itself in an artifact attached to its image (ADR 0026, Proposed). WorkSpec has no such descriptor today.
- Existing Fabric text that already references WorkSpec: `docs/architecture/git-integration-reference.md` (WorkSpec GitHub App flow, read at WorkSpec `d4a4c1d3`, which pre-dates `47607db`; re-read before relying on it), ADR 0011, ADR 0016 (`audience: workspec`), `hosting/README.md`.
- WorkSpec today: Express 5 API + React/Vite SPA (wouter router, not federated), own session cookie `workspec_session` in Postgres (`connect-pg-simple`), email/password + GitHub OAuth, mobile Bearer via `auth_tokens`, MCP tokens (`wsmcp_`) and an embedded `oidc-provider` for the MCP OAuth flow, own Stripe billing/plans, own Atrium licensing, own GitHub App provisioning, workspaces owned by a `users` row. None of these consume a Fabric-issued token or a Fabric catalogue entry yet.

Downstream Fabric issues (not to be cloned): #118 shell navigation integration; #119 identity/sessions/entitlements (links #480, #408); #120 storage and migrations (links #442, #469, #407); #121 end-to-end journey (links #493, #496, #497, #508); #122 agents/MCP/mobile boundaries (links #498, #447, #402, #412). All are Gate M4, all "Backlog, separate dispatch".

## Capability matrix

Paths are relative to the WorkSpec repo root unless prefixed `fabric:`. `API` = `artifacts/api-server`, `WEB` = `artifacts/workspec`. Test paths are under `API/test/` unless stated. The "Existing issues" column gives the state I found at head; full reasoning is in "Issues revalidated".

### 1. Workspaces and projects

| Field | Content |
|---|---|
| Capability | Workspace (tenant-like container) with slug, branding, members, invitations, seat mode (single/multi) and interaction mode (MCP vs hosted); projects inside workspaces with phase lifecycle, repo bindings, settings, per-actor branch context, presence, dashboards. |
| WorkSpec source evidence | `lib/db/src/schema/workspaces.ts` (`seatMode`, `interactionMode`), `projects.ts`, `branch-contexts.ts`, `project-repos.ts`; `API/src/routes/workspaces.ts`, `projects.ts`, `presence*.ts`, `sidebar.ts`; `API/src/services/workspaces/*` (create, invite, accept, remove-member, change-seat-mode, change-interaction-mode, check-workspace-access); `API/src/middlewares/workspace-access.ts`; `API/src/services/project-setup.ts`, `workspace-setup.ts`; WEB `pages/dashboard*.tsx`, `workspace-create.tsx`, `invite.tsx`, `project-settings.tsx`; docs `docs/v4/interaction-modes-spec.md`, `project-lifecycle-model.md`. |
| Test evidence | `routes/workspaces.test.ts` (member / non-member / cross-slug 403-not-404), `routes/idor-membership-gates.test.ts`, `services/workspaces/workspaces-service.integration.test.ts`, `services/workspaces/plan-limits.integration.test.ts`, `services/branch-context.integration.test.ts`, `routes/presence.test.ts`, `routes/presence-ws.test.ts`, `services/presence.test.ts`. No frontend tests for these pages (one WEB test file in the repo). |
| Existing issues | #408 partially mitigated (cross-tenant gates now covered for more than workspaces via `idor-membership-gates.test.ts`). #469 fixed on main (workspace-tier uniqueness, see row 6). #499 not built (retire `currentPhase` writes). #486 epic open. |
| Fabric integration gap | Workspace ownership is a WorkSpec `users` row; there is no mapping from a Fabric realm/client/tenant to a WorkSpec workspace, no provisioning hook ("assign WorkSpec to client" must create or adopt a workspace), and workspace slug routing is path-based, not host-based. Seat limits and plan limits come from WorkSpec's own `plans`/`subscriptions`, not the Fabric catalogue plan. No component descriptor (ADR 0026). |
| Owner / downstream | #119 (identity to tenant/workspace mapping), #118 (hostname and routing), #120 (storage boundary). WorkSpec owner for in-product behaviour. |
| Recommended disposition (pending Brett) | **In-launch.** Product behaviour already built; integration is the work. |

### 2. Design and C4 (diagrams, canvas, data dictionary, topology)

| Field | Content |
|---|---|
| Capability | C4 diagrams where every node is a first-class artifact (actor, external-system, container, component, database, queue); journeys; ERD/data dictionary (entities, fields); unified project canvas with groups, locks, selection, saved views; infrastructure topology with Cost/Logical/Placement lenses; artifact YAML model with pencil/ink (drafted/inked) lifecycle and draft branches. |
| WorkSpec source evidence | `lib/yaml-schemas/src/diagram.ts`, `entity.ts`, `field.ts`, `system.ts`, `actor.ts`, `external-system.ts`, `persona.ts`, `need.ts`, `feature.ts`, `scenario.ts`; `lib/db/src/schema/canvas-*.ts`, `diagram-layouts.ts`, `infra-topologies.ts`, `user-journeys.ts`; `API/src/routes/diagrams.ts`, `topologies.ts`, `journeys.ts`, `canvas-selection.ts`, `canvas-watch-ws.ts`, `artifacts.ts`; `API/src/services/diagrams/*`, `artifacts/*`, `topology-service.ts`, `canvas-aggregators.ts`; MCP `API/src/mcp/tools/canvas/*`, `topology/*`, `artifacts/*diagram*`; WEB `canvas/` (c4, diagram, graph, topology), `components/canvas-host/ProjectGraphCanvas.tsx`, `pages/project.tsx`, `topology.tsx`; docs `docs/v4/diagram-artifact-model.md`, `data-dictionary-model.md`, `docs/topology-lenses.md`. |
| Test evidence | `routes/diagram-attach.test.ts`, `routes/diagram-branch-guard.test.ts`, `services/diagrams/*`, `services/create-diagram-dsl-nodes.integration.test.ts`, `services/diagram-edge-alias.integration.test.ts`, `routes/topology-lenses.test.ts`, `mcp/topology-tools.integration.test.ts`, `mcp/get-canvas-state.integration.test.ts`, `mcp/canvas/*`, `services/canvas-selection-delete.integration.test.ts`, `services/draft-branch-awareness.integration.test.ts`, `services/branch-visibility.integration.test.ts`, `handlers-yaml-roundtrip.integration.test.ts`, `services/persona-reuse.integration.test.ts`, `services/style-compiler.test.ts`. Canvas rendering, 85k-line frontend: no tests. |
| Existing issues | #496 (canvas lens shell + pull-the-thread) **not built**: `components/shell/views/CanvasView.tsx` is 28 lines of glue over the v4 `ProjectGraphCanvas`. #437 **fixed on main** (draft-overlay search test exists, see row 9). #173, #183 (design-flow, rework halo) not verified. #417 (dark mode), #413 (no route code-splitting, Monaco + canvas in one bundle) not verified. #508 and #510 reproduce (see rows 4 and 10). |
| Fabric integration gap | The canvas is a large stateful SPA page, not a federated module: no `./Module` export, no `basePath` contract, hash/path routing owned by wouter. Canvas WebSocket endpoints (`canvas-watch-ws`, `presence-ws`) must pass the gateway same-origin and cookie model. Frontend bundle size (#413) affects module load/failure states in #118. |
| Owner / downstream | #118 (module wrapping/navigation), #121 (design step of the journey), WorkSpec owner for #496. |
| Recommended disposition (pending Brett) | v4 canvas/diagram/topology/data-dictionary: **in-launch**. #496 pull-the-thread lens shell: **in-launch if Brett wants the v5 Canvas view to be the launch canvas; otherwise explicit post-launch sequencing, not an implicit cut.** Brett decision. |

### 3. Plans and comparison

| Field | Content |
|---|---|
| Capability | The source has no feature named "plan comparison". Closest implemented capabilities: (a) delivery planner (`apply-planner`, `save-planner-draft`, `get-planner-draft`) for workflow/board plans; (b) draft-versus-main comparison: branch overlay, conflict detection, PR diff, field diff, promotion readiness; (c) cost/pricing comparison inputs: cost-catalog pricing models, environments, schedules, topology cost lens; (d) billing plan tiers (`/billing/plans`, plan limits). **Brett must confirm which of these "plans/comparison" means; this row is not a verified match to an approved capability.** |
| WorkSpec source evidence | (a) `API/src/services/workflows/apply-planner.ts`, `save-planner-draft.ts`, `get-planner-draft.ts`, `routes/workflows.ts`; (b) `API/src/services/pr-diff.ts`, `pr-conflicts.ts`, `draft-commits.ts`, `branch-visibility.ts`, `promotion.ts`, WEB `components/pull-requests/FieldDiffView.tsx`, `pages/pull.tsx`, `pulls.tsx`; (c) see row 5; (d) `API/src/routes/billing.ts`, `services/billing-quota.ts`, `stripe-service.ts`, `lib/db/src/schema/billing.ts`. |
| Test evidence | `services/workflows/workflows-service.integration.test.ts`, `services/promotion-readiness*.test.ts` (2 files), `services/branch-visibility.integration.test.ts`, `services/workspaces/plan-limits.integration.test.ts`, `routes/billing-webhook.integration.test.ts`. No tests found for `pr-diff` / `pr-conflicts` as units (not verified beyond filename search). |
| Existing issues | #410 **still reproduces** (see row 8). #415 (triage board one check-summary call per open PR): code path `services/board/get-triage-board.ts` still awaits a summary per PR; not benchmarked, treat as likely. #376 (Atlas organize-turn backend, promotion persistence) not verified in depth; no emitter of `atlas-ceremony` events was looked for beyond this note. |
| Fabric integration gap | Plan semantics collide: WorkSpec billing `plans` (workspace/seat/AI limits) versus the Fabric catalogue `planId` per assigned application (ADR 0021). One must be authoritative; today WorkSpec enforces its own. Whatever "comparison" turns out to be, no Fabric contract exists for it. |
| Owner / downstream | Brett (define capability); #119 (entitlement/plan authority); #121 (plan/compare step of the journey is named in its acceptance text). |
| Recommended disposition (pending Brett) | **Undecided until Brett defines "plans/comparison".** Recommend in-launch for (b) and (c), which are built; Brett to decide on (a) given #410. |

### 4. Decisions, registers, risks and the Thread Model

| Field | Content |
|---|---|
| Capability | Decisions as pins with who/why/date and revisit conditions; risks; registers with derived status; v5 Thread Model: threads (TH ids), derived holes, claimable holes, four-view shell (Threads, Canvas, Registers, Docs), thread detail, pin moment modal. |
| WorkSpec source evidence | `lib/yaml-schemas/src/decision.ts`, `risk.ts`, `question.ts`; `lib/db/src/schema/threads.ts`, `register-numbers.ts`; `API/src/routes/decisions.ts`, `risks.ts`, `registers.ts`, `threads.ts` (GET threads, GET thread, GET holes, POST holes/:id/claim); `API/src/services/threads/{derive-threads,derive-holes,claims,mint-th-ids,draft-link-overlay}.ts`, `registers.ts`, `register-status.ts`; migration `0069_thread_fabric.sql`; WEB `components/shell/{FourViewShell,views/ThreadsView,views/ThreadDetailView,thread-detail/*}`, `pages/decisions.tsx`, `risks.tsx`, `register.tsx`; docs `docs/v5/thread-model-concept.md`, `v4-to-thread-model-migration.md`. |
| Test evidence | `services/decisions/pin-write.integration.test.ts`, `services/threads/{derivation,claims,overlay}.integration.test.ts`, `services/registers.integration.test.ts`, `services/register-status.integration.test.ts`, `services/register-traceability.integration.test.ts`, `services/discovery-traceability.integration.test.ts`. No frontend tests for the shell or thread views. |
| Existing issues | Epic #486 open; 8 of 15 sub-issues closed (#487-#492, #500, #501: docs, Phase A derivation, four-view shell, Threads home, thread detail, pin moment: all present in source). Open: #493 registers/health **not built** (`RegistersView.tsx` is 52 lines, no ghost/repair/revisit-state UI found), #494 hole queue/claim UI **partially present** (claim API + `HoleCard.tsx` exist; no queue view with tabs, "→ atlas"/"→ client" actions or evidence-driven completion), #495 conversation dock **not verified** (no dock component found in shell), #497 docs reader **not built** (`DocsView.tsx` delegates to v4 `DocumentsContent`; no `doc_sections`/drift schema), #498 agents **not built** (see row 9), #499 the cut **not done** (promote routes/tools still present). #508 **still reproduces** (no `done_means` in `lib/yaml-schemas`; only a frontend type reads it). #509 **still reproduces** (`derive-threads.ts` emits `bound-by` only for real decisions; no `missing` node at decide). #510 **still reproduces** (`handleCreateRisk` writes `risk-of-feature`/`supersedes`/`captured-from` via `createLink`; `riskToYaml` has no `links`). |
| Fabric integration gap | #121 requires "explicitly approve a decision" and a rejected-approval path; the explicit approval maps to the pin moment (built) but its server-side authorisation (who may pin) rests on WorkSpec membership, not Fabric roles/permissions (ADR 0013/0016 declare authorisation in Fabric's words). Draft-branch correctness bugs (#509/#510) directly threaten decision integrity on drafts. |
| Owner / downstream | #121 (decision journey), #119 (who may approve), WorkSpec owner for #486 children, #508, #509, #510. |
| Recommended disposition (pending Brett) | Built Phase A/B shell, pins, threads: **in-launch**. #508/#509/#510 (correctness): **in-launch**. #493 and #497 (decisions register, compiled docs): **in-launch recommended** because #121's journey and the "record" depend on them. #494, #495, #499: Brett to decide; if sequenced after launch it must be an explicit approved decision, not an MVP cut. |

### 5. Cost

| Field | Content |
|---|---|
| Capability | Cost catalog (providers, SKUs, meters, rates, pricing models, environments, schedules, resources, criteria, entities), artifact-to-cost links, topology cost attribution, budget (`workspace_budgets`, `llm_usage`), cost engine in the web client. |
| WorkSpec source evidence | `lib/db/src/schema/cost-catalog.ts`, `operational.ts`, `infra-topologies.ts`; `API/src/routes/cost-catalog.ts`, `budget.ts`; `API/src/services/cost-catalog-service.ts`, `artifact-cost-links.ts`, `budget.ts`, `topology-service.ts`; `API/src/mcp/tools/cost-catalog/*`; WEB `pages/cost-catalog/*`, `pages/budget.tsx`, `src/lib/cost-engine.ts`; migration `0068_backfill_region_modifier_percentage.sql`. |
| Test evidence | `routes/cost-catalog.test.ts`, `mcp/cost-catalog-tools.integration.test.ts`, `mcp/artifact-cost-link-tools.integration.test.ts`, `mcp/check-catalog-consistency.integration.test.ts`, `mcp/topology-tools.integration.test.ts`; WEB `src/lib/cost-engine.test.ts` (the only frontend test file). |
| Existing issues | #477 (regionModifier unit) **fixed on main** (migration 0068 normalises to percentage; MCP description corrected per migration comment). Product-owner sign-off on the percentage direction is recorded only as a comment in the migration; the ambiguity note about values in (0,1) remains. #478 (JSONB tier/meter rates adopt numeric precision) **not verified**: `cost-catalog.ts` still has `meters: jsonb(...)`; numeric columns exist for scalar rates. |
| Fabric integration gap | Fabric has no cost/usage contract for WorkSpec. #122 requires usage persistence and quota; `llm_usage` and budgets are WorkSpec-local and unmetered by Fabric. |
| Owner / downstream | #122 (usage/quota boundary), WorkSpec owner for #478. |
| Recommended disposition (pending Brett) | **In-launch.** #478 is a data-precision correction; recommend in-launch unless Brett records that existing JSONB values are accepted. |

### 6. Persistence (database, git-backed storage, migrations)

| Field | Content |
|---|---|
| Capability | Postgres via Drizzle as query/index store, with spec artifacts as YAML in git repos (bare repos, worktrees, per-actor draft branches); DB rebuildable from git; unique constraints on artifact paths per project and per workspace; hand-registered migrations applied by a SHA-tracked runner; Fly workspace machines and local-machine agent. |
| WorkSpec source evidence | `lib/db/src/schema/*` (60+ table files), `lib/db/migrations/0000…0069` (71 files, 70 journal entries), `lib/db/migrations/meta/` (snapshots 0000-0006 and 0066-0068 only), `lib/db/src/index.ts` (pool `max: 20`, `connectionTimeoutMillis: 5000`, `idleTimeoutMillis: 30000`); `API/src/services/storage.ts`, `git-service.ts`, `workspace-storage.ts`, `repo-sync.ts`, `ingest-service.ts` (project-tier `onConflictDoUpdate`), `scratch-migration.ts`, `fly-machines.ts` (all fetches `AbortSignal.timeout(10_000)`), `local-machine.ts` (`RPC_TIMEOUT_MS = 30_000`); migration `0063` (project tier) and `0067_artifact_workspace_path_unique.sql` (workspace tier index `artifacts_workspace_path_idx` plus de-dupe). |
| Test evidence | `services/storage.integration.test.ts`, `services/git-engine.test.ts`, `services/repo-sync.test.ts`, `services/artifact-dedupe-migration.integration.test.ts`, `services/scratch-migration.test.ts`, `verification/rebuild-from-git.test.ts`, `verification/rename-handling.test.ts`, `routes/rebuild-prune.test.ts`, `services/create-artifact-slug-collision.integration.test.ts`, `handlers-yaml-roundtrip.integration.test.ts`, `global-setup.ts` (Testcontainers). No test for a failed migration/recovery path, restart persistence, or concurrent same-path ingest (#469 asked for one; I did not find it). |
| Existing issues | #469 **fixed on main** (migration 0067 present, snapshot 0067 present; the "concurrent ingest yields one row" test requested by the issue was not found, so the acceptance test is missing even though the constraint exists). #442 **partially mitigated, not verified**: snapshots for 0066-0068 now exist but 0007-0065 are still absent, so a generate against the current schema may still diff or abort; this needs a run of `pnpm --filter @workspace/db run generate`, which I did not do. #407 **partially mitigated**: deploy is now gated by a `check` job, but `.do/staging-app.yaml` still has `deploy_on_push: true` x3 and `NODE_ENV=development` on the api component, `cancel-in-progress: true` remains in `deploy.yml`, the only migration step runs `push-ci` against the CI test database, and `Dockerfile.migrate` has no references anywhere. #402 **fixed on main** (timeouts, bounded pool); no ws ping/pong heartbeat found (half-open sockets not verified). PR #484, the issue's linked fix, is still shown open; the repo state shows the changes, so link state is stale or they landed another way. #427 dual-path fallback: one "mid-rollout" site remains in `services/system-artifact.ts`; not assessed further. |
| Fabric integration gap | WorkSpec assumes Postgres it owns, a writable filesystem for git worktrees (PVC; #135 storage-model drift is cited by #408), and Fly/local machine providers. Fabric has Data Source/placement desired state (ADR 0003/0023) but nothing provisions a WorkSpec database or volume, injects `DATABASE_URL`, or runs WorkSpec migrations on deploy. Migration-on-deploy ownership is undefined (WorkSpec's Dockerfile copies migrations but does not run them at boot). Storage durability depends on the single-node local storage noted in #110's historical evidence. |
| Owner / downstream | #120 (database/service ownership, migration-on-deploy, restart durability, concurrency); WorkSpec owner for #442, #407 cleanup. |
| Recommended disposition (pending Brett) | **In-launch** and a launch blocker for the Fabric-hosted instance: durability, migration execution and recovery are acceptance items in #120. |

### 7. Export and import

| Field | Content |
|---|---|
| Capability | Document export (HTML, PDF) from templates and live artifacts; publish of branches (`publish` routes, `pdf-generator`); import by ingestion (upload a spec document, planner phases, sub-agents, `parseSpec`), rebuild-from-git, project duplicate, scratch migration, repo file routes. |
| WorkSpec source evidence | `API/src/services/documents/{export-document,assemble-document,resync-document}.ts`, `pdf-generator.ts`, `document-templates.ts`, `routes/documents.ts`, `routes/publish.ts`; `routes/ingestion.ts`, `services/ingestion-service.ts`, `ingestion-skill.ts`, `spec-parser.ts`, `routes/spec.ts`; `routes/repo.ts` (`POST /projects/:id/rebuild`); `mcp/tools/projects/duplicate-project.ts`. |
| Test evidence | `services/documents/document-create-preview-export.integration.test.ts`, `services/documents/documents-service.integration.test.ts`, `services/pdf-generator.test.ts`, `publish.integration.test.ts`, `verification/rebuild-from-git.test.ts`, `routes/rebuild-prune.test.ts`. No test file for the ingestion route or service was found. |
| Existing issues | No open issue covers whole-workspace/tenant export/import or unknown-file preservation. #497 (published record export) **not built**. #424 (production Dockerfile bundles Playwright + Chromium + Claude binary) **still reproduces** (`Dockerfile` lines installing Chromium and copying the Claude binary); this affects PDF export image size/attack surface. |
| Fabric integration gap | #121 requires export/import with "unknown imported files retain integrity" and "tenant export denial". WorkSpec has document/PDF export and spec ingestion, but I found no tenant-wide export bundle or import that round-trips unknown files, and ingestion is untested. Cross-tenant export denial is untested. |
| Owner / downstream | #121 (acceptance), WorkSpec owner (needs a product decision on a tenant export/import contract). |
| Recommended disposition (pending Brett) | Document/PDF export, publish: **in-launch**. Tenant export/import with integrity guarantees: **Brett decision required** (no issue exists; see "Gaps without an existing issue"). |

### 8. Administration (instance admin, billing, licensing, setup, support)

| Field | Content |
|---|---|
| Capability | Instance admin (users, enable/disable, reset password, workspace membership, Anthropic key, support upstream, support credentials, MCP session revocation); auth (email/password, GitHub OAuth, password reset, mobile Bearer, CSRF, rate limits); Stripe billing and plan limits; Atrium-validated license with 402 write gate; first-run setup with self-provisioned GitHub App; workflow boards/states configuration; edition (ENTERPRISE vs COMMUNITY). |
| WorkSpec source evidence | `API/src/routes/instance-admin.ts`, `auth.ts`, `billing.ts`, `setup.ts`, `github-provisioning.ts`, `github-install.ts`, `support-tickets.ts`; `API/src/services/instance-admin/*`, `licensing.ts`, `edition.ts`, `billing-quota.ts`, `stripe-service.ts`, `github-app-provisioning.ts`; `API/src/middlewares/{auth,license-gate,csrf,rate-limit,workspace-access}.ts`; `API/src/lib/env-validation.ts` (`validateProductionSecrets`, called from `index.ts`); WEB `pages/users*.tsx`, `settings.tsx`, `setup.tsx`, `workflow*.tsx`. |
| Test evidence | `services/instance-admin/instance-admin-service.integration.test.ts`, `setup/setup-flow.integration.test.ts`, `setup/provisioning.unit.test.ts`, `routes/auth-token-security.test.ts`, `routes/session-fixation.test.ts`, `routes/security-middleware.test.ts`, `routes/billing-webhook.integration.test.ts`, `routes/github-webhook.test.ts` (verification gate: no secret, invalid/missing signature, valid signature), `lib/env-validation.test.ts`, `services/workspaces/plan-limits.integration.test.ts`. **Not found:** any test importing `license-gate`, any test for `middlewares/auth.ts` disabled-account lockout as a unit, any test for MCP token scopes, github-webhook `push`/`pull_request` slice-binding. |
| Existing issues | #480 **partially mitigated**: `validateProductionSecrets()` now refuses to boot in production when `LICENSE_DEV_MODE`, `SETUP_SKIP_REACHABILITY` or `SETUP_GITHUB_APP_DEV_BYPASS` is on unless `WORKSPEC_ALLOW_INSECURE_BYPASSES=true` (tested). But `licensing.ts` `devMode()` itself still has no interlock, and `.github/workflows/deploy.yml` still writes `LICENSE_DEV_MODE=true` and `WORKSPEC_ALLOW_INSECURE_BYPASSES=true` into the ENTERPRISE env secret, so the live ENTERPRISE deploy workflow still runs with licensing disabled by design. The issue's second fix (remove it from deploy.yml before a public deployment) is not done. #408 **partially mitigated**: webhook verification gate, session fixation, auth-token security, IDOR gates, billing webhook now tested; license-gate, MCP scope enforcement, push/PR webhook branches, auth middleware lockout, frontend (1 test file), mobile (0), and API-contract tests are still absent. #447 **fixed on main** (`checkAiUsageAllowed` in `billing-quota.ts`, tested in `plan-limits.integration.test.ts`; but the only call site is `routes/conversations.ts`, so MCP/agent/dispatch AI spend paths are not gated as far as I searched). #410 **still reproduces** (`pages/workflow.tsx` still seeds `MOCK_BOARDS`/defaults and renders "coming soon" editors). #404 (no ESLint, TS not strict): **still reproduces** (no eslint config; `tsconfig.base.json` has `noImplicitAny` but no `strict`). #403 (a11y), #394 (silent failed fetches), #421 (raw `fetch('/api/…')`: 115 calls at head), #395 (route files not thin), #411 (`tool-handlers.ts` 4,580 lines): not individually verified except as counted; treat 421/411 as reproducing by measurement. |
| Fabric integration gap | Administration is duplicated: WorkSpec's instance admin, user directory, password reset, billing/Stripe, Atrium license and first-run `/setup` all overlap Fabric's control plane (operators, realm users, catalogue assignment, suspension). #121 needs "administer access, suspend/reinstate" enforced server-side; WorkSpec's `users.status` disable exists, but suspension driven by Fabric (revoked product assignment) has no inbound path. License vs entitlement is unresolved: `licensing.ts` validates an Atrium license, Fabric will express entitlement via catalogue assignment. First-run `/setup` and GitHub App self-provisioning conflict with ADR 0011 (platform creates its own git application). |
| Owner / downstream | #119 (identity, entitlement, license reconciliation with #480, suspend/reinstate), WorkSpec owner for #408, #410, #404; Brett for Stripe/Atrium vs Fabric authority. |
| Recommended disposition (pending Brett) | Admin, auth, security tests: **in-launch**; #480 deploy.yml bypass removal and `devMode()` interlock: **in-launch, launch blocker**. Stripe and Atrium inside Fabric-hosted instances: **Brett decision** (keep, replace by catalogue entitlement, or disable per instance). #410 workflow-board configuration: **Brett decision** (implement vs remove the routes until backed; the issue allows either). |

### 9. Agents, MCP and mobile

| Field | Content |
|---|---|
| Capability | MCP server at `/mcp` (182 tool registrations across 197 files at head; per-workspace Bearer tokens `wsmcp_…`, scopes concierge/advisor, embedded `oidc-provider` OAuth, MCP sessions with revocation); hosted Atlas and specialist agents (architect, concierge, dev, pa, tester); managed agents via agent bus; Fly workspace-agent (gRPC) and local `workspec-connect` tunnel; mock-agent dev loop; Playwright screenshot/test runs; thread/hole tools (claim); Expo mobile app (tabs: home, delivery, notifications, settings; conversation; register). |
| WorkSpec source evidence | `API/src/mcp/{index,server,auth,token-scopes,agent-bridge,tool-helpers,zod-to-anthropic}.ts`, `mcp/tools/{artifacts,boards,canvas,conversation,cost-catalog,delivery,integration,operations,projects,sessions,topology}/*`; `routes/mcp-sessions.ts`, `mcp-tokens.ts`, `machine-agent-ws.ts`, `terminal.ts`, `project-machines.ts`; `API/src/agents/*`, `services/{managed-agent,hosted-atlas,dispatch-agent,sub-agent,fly-machines,local-machine,workspace-machine,agent-tools}.ts`, `src/dispatch/*`; `lib/agent-bus`, `lib/workspace-proto`, `artifacts/{workspace-agent,connect-agent,mock-agent-mcp,mobile}`; `routes/_adapter.ts` (REST to MCP adapter). |
| Test evidence | `mcp/tool-registry.test.ts` (registration smoke test only), `mcp/read-access.integration.test.ts`, `mcp/list-workspace-members.integration.test.ts`, `mcp/list-tools-structured-content.integration.test.ts`, `mcp/error-surfacing.test.ts`, `mcp/input-schema.test.ts`, `mcp/slug-resolver.integration.test.ts`, `mcp/search-artifacts*.integration.test.ts`, `mcp/rename-artifact-missing-file.integration.test.ts`, `mcp/screenshot-path-validation.test.ts`, `mcp/playwright-service-same-origin.test.ts`, `mcp/zod-to-anthropic.test.ts`, `e2e/agent-journey.test.ts` (in-process blank project to dispatched slice; mocks LucidBrain SDK and GitHub), `services/mock-agent/*`, `dispatch/{composeIssueBody,createIssue}.test.ts`, `services/threads/claims.integration.test.ts`. No tests for `workspace-agent`, `connect-agent`, `mock-agent-mcp`, `lib/*`, or `artifacts/mobile`. |
| Existing issues | #498 (MCP hole tools, evidence pipeline, Atlas temperament) **not built**: no `list_holes`, `get_thread`, `attach_evidence` tools; no evidence table; `list_pending_turns` still exists; no temperament setting found. Claims API exists (`POST /projects/:id/holes/:holeId/claim`) and is tested. #412 **still reproduces** (`routes/_adapter.ts` `classifyStatus` regexes the error message; `tool-handlers.ts` has 41 `throw new Error`). #402 **fixed on main** except ws heartbeat not found (row 6). #447 **fixed on main** with call-site caveat (row 8). #408 MCP behavioural coverage and scope-enforcement tests **still absent**. #437 **fixed on main** (`search-artifacts-draft-overlay.integration.test.ts`). #481 (create_feature silent slug coalescing) **fixed on main** (`create-artifact-slug-collision.integration.test.ts` asserts rejection before write; the MCP create_feature path itself was not traced). #428 (mobile Expo 54/55 mix) not verified. #376 not verified. |
| Fabric integration gap | The MCP surface authenticates with WorkSpec-minted tokens and its own `oidc-provider` issuer (`OAUTH_ISSUER`), not Fabric realm tokens; a Fabric-hosted MCP endpoint on the tenant hostname needs the gateway/token model of ADR 0019/0024 to either front or replace this. Agent execution (Anthropic key, Fly machines, local tunnels, GitHub App credentials) has credential ownership inside WorkSpec; #122 needs these bounded per tenant with usage/quota and cancellation. Mobile uses Bearer `x-client-type: mobile` and a bundled landing server (`mobile/server/serve.js`); no Fabric mobile identity flow exists, and there are zero mobile tests. |
| Owner / downstream | #122 (all boundaries), #119 (token/session issuance), WorkSpec owner for #498, #412. |
| Recommended disposition (pending Brett) | MCP server, agents, mobile app as built: **in-launch** (Enterprise capability set, per #122 "approved Enterprise agent/MCP/mobile capability set"). #498 hole tools + evidence pipeline: **in-launch recommended** (the epic's own "done means" loop depends on it); Atlas temperament "night shift": **Brett decision** (its own "out of scope" note defers the code-writing permission envelope). |

### 10. Integrations

| Field | Content |
|---|---|
| Capability | GitHub App (self-provisioned org App, install flow, webhooks, PR-to-slice binding via `Workspec-Slice:` trailer, push re-ingest, repo creation); GitHub OAuth login; Stripe billing; Atrium licensing service; Fly Machines API; Anthropic API (managed agents); Playwright test runs reporting to `/test-runs/from-ci`; support-ticket intake; GitHub Actions workflows (CI, deploy, Playwright UX slices, verification harness, mobile build/update). |
| WorkSpec source evidence | `API/src/services/{github-app,github-app-provisioning,github-install,github-service,pull-requests,pr-service,pr-merge-service,pr-review-service}.ts`, `routes/{github-webhook,github-install,github-provisioning,setup,pulls,tests}.ts`; `stripe-service.ts`; `licensing.ts`; `fly-machines.ts`; `support-intake.ts`, `routes/support-tickets.ts`, `support-mcp.ts`; `.github/workflows/{ci,deploy,playwright,verification-harness,mobile-build,mobile-update}.yml`; docs `docs/github-app-setup.md`, `docs/v4/self-provisioning-github-app-as-built.md`. |
| Test evidence | `routes/github-webhook.test.ts` (verification gate only), `setup/setup-flow.integration.test.ts`, `setup/provisioning.unit.test.ts`, `routes/billing-webhook.integration.test.ts`, `setup/license-fixtures.ts`, `services/board/*`, `dispatch/*`, `routes/health.test.ts`. No tests for push/pull_request handling or HMAC helper beyond the gate. |
| Existing issues | #408 (push and `pull_request` to slice binding untested) **still reproduces**. #407 **partially mitigated** (row 6). #480 (row 8). #423 (`@tldraw/sync-core` in api-server production deps under the non-OSS licence): not re-checked, because `grep` of `artifacts/api-server/package.json` and `artifacts/workspec/package.json` for `@tldraw` returned nothing at head, which suggests it is **fixed on main or moved**; confirm against the lockfile before closing. #429 (CI hygiene, secret interpolation into shell) and #422 (dependency override floors): not verified. |
| Fabric integration gap | GitHub App flow is documented in Fabric as a reference, not as an integration: ADR 0011 says the platform creates its own git application and rejects a `connect-existing` path, while WorkSpec offers per-instance self-provisioning with secrets encrypted into its own DB (`WORKSPEC_ENCRYPTION_KEY`). Secret boundary (ADR 0017) is not applied to WorkSpec's stored credentials. Inbound webhooks (`/api/webhooks/github`, `/api/billing/webhook`) need ingress routes on tenant hostnames that bypass gateway sign-in. WorkSpec's deploy workflows (k3s, DigitalOcean spec) are not the Fabric deployment path. |
| Owner / downstream | #119 (credential/secret ownership), #122 (external timeouts/credentials), #120 (deploy-time behaviour), WorkSpec owner for #408, #407. |
| Recommended disposition (pending Brett) | **In-launch** for GitHub/Anthropic/Fly integrations in their Fabric-adapted form; Stripe/Atrium per row 8 decision. |

### 11. Cross-cutting: shell federation and navigation

| Field | Content |
|---|---|
| Capability | The user-facing navigation: v4 project sidebar (15+ items under `/project/:id/...`: refinement, backlog, board, decisions, risks, triage, attention, testing, velocity, reconcile, pulls, documents, repository, personas/domains/features, discovery, prototype, settings) plus v5 four-view shell (`pages/shell.tsx`), landing/login/consent/invite, workspace/instance admin pages. |
| WorkSpec source evidence | `WEB/src/App.tsx` (wouter `Route` table), `pages/shell.tsx`, `components/shell/*`, `sidebar/` in API (`routes/sidebar.ts`, `src/sidebar`). |
| Test evidence | `routes/sidebar.test.ts` (API only). No WEB route/navigation tests. |
| Existing issues | #499 (collapse nav to four views, delete the 15-item sidebar) **not done**. #417, #413, #403, #394 (dark mode, code-splitting, a11y, silent errors) see rows 2 and 8. |
| Fabric integration gap | Directly #118: Module Federation entry (`./Module`), `basePath` prefix handling for wouter, same-origin asset paths under `/modules/workspec/`, no token in browser (WorkSpec keeps its own cookie session and OAuth redirect pages), `401` to gateway sign-in rather than WorkSpec `/login`, WorkSpec `/setup`, `/register`, `/forgot-password` flows must not be reachable or must be Fabric-owned, module compatibility/manifest and error states. Plus `docs/` design handoff rule from #118: new visual design needs a fresh approved handoff. |
| Owner / downstream | #118. |
| Recommended disposition (pending Brett) | **In-launch.** The whole navigation (v4 sidebar and v5 four-view) must be reachable per #118's acceptance; Brett to decide whether v4 sidebar pages stay until #499 lands. |

## Issues revalidated

State is the issue state on GitHub on 2026-10-05; "verdict" is my finding at WorkSpec `47607db`.

### The 15 issues linked from Fabric #110

| # | Title (short) | State | Verdict at head | How checked |
|---|---|---|---|---|
| 486 | Epic: Thread Model (v5) | open | **Open, partly delivered.** 8/15 sub-issues closed; remaining are #493-#499. | Sub-issue list via GitHub; source for the closed items present (threads routes/services/tests, shell, pin moment). |
| 497 | ui(docs): compiled reader, drift, published record | open | **Not built.** | `DocsView.tsx` wraps v4 `DocumentsContent`; no `doc_sections` / drift schema or compile service. |
| 496 | ui(canvas): lens shell + pull-the-thread | open | **Not built.** | `CanvasView.tsx` 28 lines hosting v4 `ProjectGraphCanvas`; no lens picker or thread strip. |
| 495 | ui(conversation): dock + precipitation review | open | **Not verified; no dock component found.** | grep of shell components only; v4 conversation UI exists elsewhere. |
| 494 | ui(work): hole queue + claim flow | open | **Partially present.** Claim API + `HoleCard` exist; queue view with tabs, client routing, evidence-driven completion not found. | `routes/threads.ts`, `services/threads/claims.ts`, `HoleCard.tsx`, `claims.integration.test.ts`. |
| 493 | ui(registers): decisions register + health repair queue | open | **Not built.** | `RegistersView.tsx` 52 lines; no ghost/repair/revisit-state UI found. |
| 498 | agents: MCP hole tools, evidence pipeline, Atlas temperament | open | **Not built** (claim REST exists). | No `list_holes`/`get_thread`/`attach_evidence` tool; no evidence table in `lib/db/src/schema`; `list_pending_turns` still present. |
| 508 | api(threads): done-means field on features | open | **Still reproduces.** | No `done_means` in `lib/yaml-schemas`; `featureToYaml` unchanged; only `artifact-content-types.ts` references it. |
| 509 | api(threads): missing bound-by node at decide | open | **Still reproduces.** | `derive-threads.ts` L545-553: `bound-by` pushed only per real decision; no missing node when none. |
| 510 | api(risks): risk links into YAML | open | **Still reproduces.** | `tool-handlers.ts` L1036-1058 `createLink` for `risk-of-feature`/`supersedes`/`captured-from`; `riskToYaml` has no `links`. |
| 480 | LICENSE_DEV_MODE no prod interlock | open | **Partially mitigated; the live-deploy part still reproduces.** | `env-validation.ts` refuses prod boot unless acknowledged (test exists); `licensing.ts devMode()` unchanged; `deploy.yml` L227-235 sets both flags for ENTERPRISE. |
| 469 | workspace-scoped artifact uniqueness | open | **Fixed on main (issue stale).** Acceptance test for concurrency not found. | Migration `0067_artifact_workspace_path_unique.sql` + `0067_snapshot.json`; `artifact-dedupe-migration.integration.test.ts` exists (not read in detail). |
| 442 | drizzle-kit generate broken | open | **Not verified; likely partly improved.** | Snapshots now 0000-0006 and 0066-0068; 0007-0065 still absent. `generate` not run (not installed). |
| 408 | Risk-critical paths untested | open | **Partially mitigated.** Remaining gaps listed in row 8. | Test-file inventory and greps (`license-gate`, `token-scopes`, `requireScope`, `Workspec-Slice`, `openapi`, `api-zod` returned no test hits except as noted). |
| 407 | Deploy pipeline correctness | open | **Partially mitigated.** | `deploy.yml` has gating `check` job; `.do/staging-app.yaml` `deploy_on_push: true` x3 and `NODE_ENV=development`; `cancel-in-progress: true`; no `Dockerfile.migrate` references; no deploy-time migration step. |

### Other open WorkSpec issues examined (relevant to Fabric integration)

| # | Title (short) | State | Verdict at head | How checked |
|---|---|---|---|---|
| 499 | The cut: retire promote/rooms/phase gates | open | **Not done, correctly open.** | `routes/promotion.ts`, `promote_project` tool, `project-phase-gates.ts` present. |
| 447 | Meter AI usage vs aiUsageLimit | open | **Fixed on main (issue stale), with a coverage caveat.** | `checkAiUsageAllowed` + `llm_usage`/`logLlmUsage`; test in `plan-limits.integration.test.ts`; only call site is `routes/conversations.ts`. |
| 402 | External call timeouts, pg pool | open | **Fixed on main (issue stale), heartbeat not found.** | `fly-machines.ts`, `local-machine.ts` `RPC_TIMEOUT_MS`, `lib/db/src/index.ts` pool options. PR #484 still listed open. |
| 412 | REST-to-MCP adapter regex status | open | **Still reproduces.** | `routes/_adapter.ts` `classifyStatus`; 41 `throw new Error` in `tool-handlers.ts`. |
| 410 | Workflow boards mock-only UI | open | **Still reproduces.** | `pages/workflow.tsx` `MOCK_BOARDS`, "coming soon" strings. |
| 437 | Branch-aware search/resolve | open | **Fixed on main (issue stale).** | `mcp/search-artifacts-draft-overlay.integration.test.ts` covers draft-only artifacts. Resolve path not separately traced. |
| 477 | regionModifier unit ambiguous | open | **Fixed on main (issue stale).** | Migration `0068` + `0068_snapshot.json`; schema comment. |
| 478 | Cost JSONB rates numeric precision | open | **Not verified.** | `meters: jsonb` still in `cost-catalog.ts`. |
| 481 | create_feature slug collisions silent | open | **Fixed on main for `createArtifact`; MCP path not traced.** | `create-artifact-slug-collision.integration.test.ts`. |
| 421 | Raw `fetch('/api/…')` calls | open | **Still reproduces** (115 calls). | grep count. |
| 424 | Production image bundles Playwright + Chromium + Claude binary | open | **Still reproduces.** | `artifacts/api-server/Dockerfile`. |
| 404 | No ESLint, TS not strict | open | **Still reproduces.** | No eslint config; `tsconfig.base.json` lacks `strict`; CI has a `TODO(#404)`. |
| 411 | God files | open | **Still reproduces for `tool-handlers.ts`** (4,580 lines). | `wc -l`; `auth.ts` not measured. |
| 423 | tldraw sync-core under non-OSS licence | open | **Likely fixed or moved; verify lockfile.** | grep of package.json files returned no `@tldraw`. |
| 415 | Triage board per-PR check fan-out | open | **Likely reproduces, not measured.** | `get-triage-board.ts` awaits `loadCheckSummary` per PR. |
| 427 | Legacy dual-path fallbacks | open | **Not verified; one "mid-rollout" site remains.** | `services/system-artifact.ts`. |
| 376 | Atlas organize-turn backend + promotion persistence | open | **Not verified.** | Read only. |
| 394, 395, 403, 413, 417, 420, 422, 425, 426, 428, 429, 386 (BA backlog), 307 (go-live scope transfer), 183, 173 | various | open | **Not verified.** | Titles/labels only; no titles used as proof. |

## Gaps without an existing issue

Proposed titles only. No issues were created.

1. `[WorkSpec] Define and prove the tenant export/import contract: whole workspace/project bundle, unknown files preserved, cross-tenant denial` (row 7; needed by Fabric #121).
2. `[WorkSpec] Test coverage for ingestion (spec upload, planner, rebuild) and workspace uniqueness under concurrent ingest` (row 6/7; #469 acceptance test missing).
3. `[WorkSpec] License-gate, auth-middleware lockout and MCP token-scope integration tests` (row 8; carved out of #408 so #119 can reference a concrete deliverable).
4. `[WorkSpec] Gate every AI spend path (MCP, dispatch, managed agents) with checkAiUsageAllowed, not only the conversations route` (row 8/9; #447 stale-open but under-covered).
5. `[WorkSpec] Remove LICENSE_DEV_MODE and WORKSPEC_ALLOW_INSECURE_BYPASSES from the ENTERPRISE deploy workflow and add a devMode() production interlock` (row 8; the unfinished half of #480).
6. `[WorkSpec] Module Federation entry for the WorkSpec SPA: ./Module, basePath-aware routing, no standalone auth pages` (row 11; #118 will consume it, kept distinct from #118's shell work).
7. `[WorkSpec] Accept a Fabric-issued bearer / trusted-ingress identity alongside session cookies and map realm, subject and roles to workspace membership` (row 1/8; #119 adapter contract on the WorkSpec side).
8. `[WorkSpec] Server-side suspend/reinstate and revoked-product-assignment handling (instance-level), including session and MCP token invalidation` (row 8; #119 and #121).
9. `[WorkSpec] Component descriptor (ADR 0026) and provisioning contract: database, storage volume, migration-on-deploy, health endpoints` (row 6; #120).
10. `[WorkSpec] Add a WebSocket liveness heartbeat for the local-machine and presence connections` (row 6/9; remaining part of #402 if still wanted).
11. `[WorkSpec] Mobile: smoke tests for bearer auth and one data fetch; reconcile Expo SDK versions` (row 9; #408 sub-item, #428).
12. `[WorkSpec] API contract tests validating responses against openapi.yaml / api-zod` (row 8; #408 sub-item, not covered by any downstream Fabric issue).
13. `[Fabric] Decide authority between WorkSpec billing/Stripe/Atrium license and Fabric catalogue plan/entitlement` (row 3/8; Brett decision, likely belongs under #119).
14. `[Brett] Define "plans/comparison" as an in-product capability` (row 3; the acceptance text names it but the source has no feature of that name).

## Items not verified

- No test was executed; all test evidence is file existence and, in a few cases, reading the file's header. CI at an exact head is needed for pass status.
- `drizzle-kit generate` (#442) was not run.
- WorkSpec issue comments were not read except via issue bodies; #437 shows one comment that was not read.
- Open PRs on WorkSpec were not inspected (including #484, linked from #402). Fabric open PRs were not inspected either, so overlap with in-flight shell/federation work (#118) is unchecked; #118 itself says to inspect them before implementing.
- Whether a conversation dock (#495) or Atlas ceremony emitter (#376) exists was judged by filename/grep only.
- The mobile app, `workspace-agent`, `connect-agent` and `mock-agent-mcp` were inventoried but not read in depth.
- The Fabric reference doc `git-integration-reference.md` was read at WorkSpec `d4a4c1d3`; differences against `47607db` were not diffed.
- Deployment/cluster state was not examined; #110's historical Oct3 observations were not used.
