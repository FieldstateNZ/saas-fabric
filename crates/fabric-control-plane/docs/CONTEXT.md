# fabric-control-plane — LLM context

The operator-facing control plane. Depends on `fabric-core`,
`fabric-client-model`, `fabric-reconciliation`, `axum`, `http`, `serde`,
`serde_json`, `thiserror`, `tokio`, `tracing`. Event domain `10`.

**No dependency on any adapter.** The Git and Keycloak crates depend inward on
this one's ports; only the composition root sees them.

## Public surface

- `build_control_plane(&ControlPlaneConfig, ControlPlaneDeps) -> Result<ControlPlaneServices, String>`;
  `ControlPlaneServices { router, statuses, health, platform_sweeps, publisher,
  publication }`. Starting the reconciliation loop, the platform sweep, and the
  publication schedule (`publisher`/`publication`, ADR 0023 part 4) are all
  **the host's to do**, not this function's -- it only builds what each needs
  and hands it back.
- `ClientRepository` (async trait) — `list()`, `get(&ClientId)`,
  `update(&ClientId, &ClientDocument, &ClientRevision, &ChangeContext) -> ClientRevision`,
  `describe()`. **No `create`, no `delete`** — both absent deliberately.
- `StoredClient { document: ClientDocument, revision: ClientRevision }` — the
  pairing *is* the concurrency mechanism; do not split them.
- `ChangeContext { requested_by: String, summary: String }` — attribution the
  repository records. Never a credential.
- `RepositoryError` — `NotFound{client}`, `Conflict`, `Unavailable{detail}`,
  `NotPermitted`, `Rejected{detail}`, `Invalid{client, source}`.
- `InMemoryClientRepository` — `new()`, `insert(ClientDocument) -> Result<ClientRevision, _>`,
  `set_unavailable(Option<String>)`. Implements the concurrency rule, not a
  shortcut past it.
- `ClientService::new(repository, statuses, trigger, clock)` — `list()`, `get()`,
  `set_identity(&Operator, &ClientId, IdentityConfiguration, &ClientRevision)`.
- `Operator` — `subject()`. Axum extractor via `FromRequestParts`. No
  constructor outside this crate.
- `OperatorAuthenticator` (trait) — `authenticate(&HeaderMap) -> Result<Operator, OperatorAuthError>`,
  `describe()`. `TrustedHeaderOperators::new(header, &[String])` is the only
  implementation; an empty allowlist is refused at construction.
- `OperatorAuthError` — `Missing`, `NotAnOperator`. The presented subject is
  never echoed back and never logged.
- `ControlPlaneError` — `Unauthenticated`, `UnknownClient`, `InvalidRequest`,
  `InvalidDesiredState`, `RevisionRequired`, `RevisionConflict`, `RealmImmutable`,
  `RepositoryUnavailable`, `RepositoryDenied`, `RepositoryRejected`.
  `status()`, `code()`, `public_message()`, `IntoResponse`.
  `from_repository` is **not** a `From` impl, so a call site cannot forward a
  repository `detail` to the browser by accident.
- `ControlPlaneConfig { operator: OperatorConfig, reconciliation: ReconciliationConfig }`.
  No `Default`: the operator posture has no safe one.
- `ReconciliationLoop::spawn(...) -> ReconciliationLoopHandle`;
  `ReconciliationTrigger::{new, request_pass}`.
- `API_PREFIX = "/api"`.
- `PlatformBinding { service, environment, repository, data_sources,
  placements, publisher, publication }` (ADR 0023) -- `publisher:
  Option<Arc<RuntimePublisher>>` is `Some` only when both a platform and a
  publication target are configured; `publication: Arc<PublicationState>` is
  always present, so `GET /api/platform`'s row can be `None` from `publisher`
  alone, with no `Option<Option<_>>`. `POST /api/platform/publication`
  (`handlers/platform/publish.rs`) runs `publisher.publish_once` and renders
  the response from the outcome it just returned, never a second
  `PublicationState` read (a scheduled pass could complete in the gap and
  make a stale read describe a pass nobody asked for).

- Image registries (ADR 0026 section 5), `registries/`: `RegistryService`
  (`new(RegistryServiceParts { store, secrets, connector, clock, deployment })`,
  `restore() -> bool` — `false` when a store did not answer — and
  `restore_until_complete()`, which the composition root spawns then), required in `ControlPlaneDeps.registries` whether or not a
  platform is managed. Ports: `RegistryStore` (the whole record set; nothing
  recorded is an empty set, an unparseable set is `Malformed`),
  `RegistryConnector` (`connect(RegistryConnection) -> Arc<dyn
  RegistryClient>`, `install(BTreeMap<RegistryHost, _>)`, implemented in the
  composition root) and `RegistryClient` (`prove`, `prove_repository` and
  `version_tags` answering `Readability::{Readable, NotReadable}` for a
  `401`/`403`/`404`, `credential_refused`). `RegistryCredential` carries its
  stored credential's refusal mark (`marked_by`, `refusal_mark`), one per
  `SecretId`, held in the service (`service/marks.rs`) so every client built
  from one credential shares it. The mark is this process's memory only: a
  restart, or a successor pod, starts unmarked, so a later read or proof may
  present the credential again, and the refusal message says so. A token goes
  through `SecretStore`
  as `integrations/registries/<SecretId>/credential`, the id server-minted
  and re-validated on read. `RegistryHost`, `RegistryKind` (`ghcr`,
  `dockerHub`, `distribution`), `RegistryRecord`, `DeploymentRegistry`,
  `InMemoryRegistryStore`, `RegistryFailure` (inside
  `ControlPlaneError::Registry`; codes in `errors/status_mapping/registry.rs`).
  Routes in `routes/registries.rs`; change handlers take their extractors as
  `Result` and audit a refusal through `handlers/registries/admitted.rs`;
  audit `control_plane.audit.registry` (S14); restore warnings W6, W7.

## Hard invariants — do not break

1. **No handler may reach an identity provider.** `ControlPlaneState` holds
   the client domain service and the operator authenticator; no handler holds
   a Keycloak (or other provider) client, and the reconciliation loop is the
   only thing that talks to one (ADR 0008, §8). This does **not** forbid
   reaching *platform* services: handlers read and act on `PlatformManagement`
   and `RuntimePublisher` through `PlatformBinding` (component versions,
   rollback, data sources, placements, publication) -- none of that is an
   identity-provider call, and the rule was never about avoiding a service,
   only about avoiding Keycloak specifically.
2. **`set_identity` checks the revision before the no-op short-circuit.**
   Otherwise `If-Match` means "unless it does not matter".
3. **A write marks reconciliation `Pending` before anything else runs**, and
   regardless of whether the loop ever does. Status must be honest from the
   instant the write lands.
4. **A realm change is refused.** Reconciliation only adds, so a rename would
   create an empty realm and abandon the one holding every user and session.
5. **Every handler takes an `Operator`.** Removing the parameter makes the
   endpoint public; there is no other check.
6. **Repository `detail` never reaches a response.** It may name a branch, a
   path, or an upstream body.
7. **`If-Match` refuses `*`, weak tags, and lists.** Each is a way to opt out
   of concurrency control.
8. **A registry change is proven before it is recorded, and written
   credential first, record second**; removal runs live client, record,
   credential. Every change takes the one `tokio` mutex in a spawned task and is
   audited, refusals included. That mutex is process-local and the record set
   is read and written whole, so one desired replica does not exclude two
   writers while a rolling update overlaps old pod and new; each would hold
   its own refusal marks too. Preventing that overlap — a single writer by
   drain and verified termination, or coordination between writers, with its
   availability cost approved — is an activation gate ADR 0026 section 5
   leaves open, not a design this crate has chosen, and a replica count or a
   drain alone is not fencing. No view carries a token; no error, log or audit
   line carries a token or a username; and a registry failure never maps to
   the platform's codes. A change never rebuilds a client with a fresh
   refusal mark for a credential it did not replace.

## Design notes

- `reconciliation_view::resolve` reports `pending` when the recorded report is
  for a *different* revision, and drops that report's failure detail with it. A
  green tick over stale information is the failure this prevents; a stale error
  message beside fresh state is the other half of it.
- Writing an unchanged identity returns the current state without writing: a
  no-op commit would reset a converged client to `pending` and put an empty
  change in the audit trail.
- `GET /api/platform`'s versions-not-selected rows
  (`handlers/platform/body/diagnostics.rs` + `diagnostics/{state,found}.rs`)
  are `{version, state}` with a closed `DiagnosticState` — `publishing`,
  `undescribed`, `incoherent`, `invalid` — and only `invalid` carries a
  `reason`, `InvalidReason::code()` from `fabric-platform-management` (ADR
  0026 section 9), and beside `unsupportedVersion` alone a `found` format
  version (`v` and at most fifteen digits, else left out), because ADR 0026
  section 2 has a reader name the version it found. `DiagnosticRow::every`
  destructures `Diagnostics` so a new list cannot go unrendered. A described
  component reports `artifact: "oci"`, and its rollback candidates
  (`handlers/platform/rollback/candidates.rs`) name `source_revision` as an
  image component's do — never the component descriptor's digest.
- `audit` is a separate module from `logging` because the two have different
  audiences and retention expectations. Git history is a second copy of the
  trail, not the whole of it — a refused write leaves no commit.
