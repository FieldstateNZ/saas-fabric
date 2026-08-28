# fabric-control-plane — LLM context

The operator-facing control plane. Depends on `fabric-core`,
`fabric-client-model`, `fabric-reconciliation`, `axum`, `http`, `serde`,
`serde_json`, `thiserror`, `tokio`, `tracing`. Event domain `10`.

**No dependency on any adapter.** The Git and Keycloak crates depend inward on
this one's ports; only the composition root sees them.

## Public surface

- `build_control_plane(&ControlPlaneConfig, Arc<dyn ClientRepository>, Arc<dyn Clock>)
  -> Result<ControlPlaneServices, String>`; `ControlPlaneServices { router, statuses, trigger }`.
  The reconciliation loop is **the host's to start**, not this function's.
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

## Hard invariants — do not break

1. **No handler may reach a platform service.** `ControlPlaneState` holds the
   service and the authenticator, and nothing else. The reconciliation loop is
   the only thing that talks to a provider.
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

## Design notes

- `reconciliation_view::resolve` reports `pending` when the recorded report is
  for a *different* revision, and drops that report's failure detail with it. A
  green tick over stale information is the failure this prevents; a stale error
  message beside fresh state is the other half of it.
- Writing an unchanged identity returns the current state without writing: a
  no-op commit would reset a converged client to `pending` and put an empty
  change in the audit trail.
- `audit` is a separate module from `logging` because the two have different
  audiences and retention expectations. Git history is a second copy of the
  trail, not the whole of it — a refused write leaves no commit.
