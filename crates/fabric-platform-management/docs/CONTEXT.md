# fabric-platform-management — LLM context

Decides which version of a platform component an environment should run:
channels, update policy, discovery, rollback, the late-bound connection to
the currently-live platform repository, and (since PR #72, ADR 0022) an
optional read-only bridge to independently observed deployment evidence. In
neither plane (see `docs/architecture/crate-dependencies.md`) — no
transport, no HTTP, no Git, no Kubernetes client. Depends only on
`fabric-core`, plus `async-trait`, `serde`, `thiserror`, `time`, `tokio`
(declares `sync` and `rt`; links the workspace's additive `macros`,
`rt-multi-thread`, `sync`, `time` set, as its Cargo.toml says), `tracing`.

## Public surface (all re-exported from `lib.rs`)

- `ArtifactSource` — `Oci { repositories: BTreeMap<String, String> }` |
  `Helm { repository: String, chart: String }`. `.kind() -> ArtifactKind`.
- `ArtifactKind` — `Oci` | `Helm`. What the console is told to word a
  rollback's guarantee; not a `rollable: bool`.
- `Release` — `Unit(ReleaseUnit)` | `Chart { repository, chart, version }`.
  `.version() -> &Version`.
- `Version` — `SemVer`-precedence `Ord`/`PartialOrd`; hand-written
  `PartialEq` = same precedence (build metadata ignored, matching `SemVer`).
  `as_str()`, `channel() -> Channel`, `is_series(&self, series: &Self) ->
  bool` (same `major.minor.patch` core only). `parse`/`parse_chart` in
  `version/parse.rs` (chart grammar keeps build metadata in the tag; OCI
  grammar refuses it — an OCI tag cannot legally carry `+`).
- `Channel` — `Preview` | `Stable` (`serde`, `rename_all = "lowercase"`).
- `UpdatePolicy` — `Automatic` | `Manual` | `Locked` (`serde`,
  `rename_all = "lowercase"`).
- `Hold { reason: String, since: String, note: Option<String> }` — no
  version field; the desired version already *is* the held one.
- `PlatformDesiredState` — the late-bound binding.
  `PlatformDesiredState::unconnected() -> Arc<Self>`. `async fn
  is_connected(&self) -> bool`. `async fn connect(&self, repository: Arc<dyn
  DesiredState>)`, `async fn unusable(&self, detail: &str)`, `async fn
  disconnect(&self)` — all wait for every in-flight operation against the
  *previous* binding (drain). Implements `DesiredState` itself, so it is
  interchangeable with the port it wraps.
- `DesiredState` (async trait) — `components(environment) ->
  Vec<String>`; `component(environment, component) -> ComponentDesired`;
  `advance(environment, component, release, at: &DesiredRevision, message)`;
  `roll_back(environment, component, release, hold: &Hold, at, message)`;
  `pause(environment, component, hold: &Hold, at, message)`;
  `resume(environment, component, at, message)`. Every write returns
  `Result<(), DesiredStateError>` and refuses `Conflict` if `at` no longer
  matches. Contract requires bounding *starting* new requests, never
  cancelling one in flight.
- `ComponentDesired { version, channel, policy, hold: Option<Hold>, source:
  ArtifactSource, revision: DesiredRevision }`.
- `DesiredRevision` — opaque; `new(impl Into<String>)`, `as_str()`. Never
  parsed by a caller.
- `DesiredStateError` — `NotConnected`, `NotFound { what }`, `Conflict`,
  `Unavailable { detail }`, `Refused { detail }`.
- `Registry` (async trait, image discovery port) — `tags(repository) ->
  Vec<String>`; `resolve(repository, tag) -> Option<Resolved>` (absence is
  not an error — a version publishing to some but not all of a component's
  repositories is an ordinary window).
- `Resolved { digest: String, provenance: Provenance }`.
- `Provenance` — `Agreed(String)` | `Absent` | `Disagreed` (three states,
  not two — absence and disagreement need different responses: wait, vs.
  never resolves).
- `RegistryError` — `Unavailable { detail }` | `Refused { detail }`.
- `ChartIndex` (async trait, chart discovery port) — `versions(repository,
  chart) -> Vec<Version>`. Implementations must refuse (not silently accept)
  two entries of equal `SemVer` precedence.
- `DeploymentObserver` (async trait, ADR 0022, `observation.rs`) — `async fn
  observe(&self, environment: &str, component: &str) ->
  Option<DeploymentObservation>`. `None` means this component has no
  observation binding, not that observation failed (a failed-but-configured
  read comes back as `Some(DeploymentObservation { health:
  DeploymentHealth::Unavailable, .. })`). Optional: `PlatformManagement`
  works with none attached, exactly as it did before this port existed.
- `DeploymentObservation { observed_at_unix_seconds: u64, version:
  Option<String>, health: DeploymentHealth, workloads:
  Vec<WorkloadObservation>, detail: Option<String> }` (`Serialize`,
  `PartialEq`). `version` is `Some` only when every active workload is
  healthy and agrees; `detail` is a controlled explanation, never a provider
  response body.
- `WorkloadObservation { name: String, health: DeploymentHealth, versions:
  Vec<String>, desired_replicas: Option<u32>, ready_replicas: u32, detail:
  Option<String> }` (`Serialize`, `PartialEq`).
- `DeploymentHealth` — `Healthy | Progressing | Degraded | Stopped |
  Unavailable` (`Copy`, `Serialize`, `rename_all = "camelCase"`).
- `Discovery { newer: Option<Release>, not_yet: Vec<Version>, incoherent:
  Vec<Version> }` (`Default`). `discover`, `history`, `resolve` (OCI);
  `discover_chart`, `chart_history`, `resolve_chart` (Helm) — all
  `pub use discovery::...` at crate root, all only ever look *above* a
  floor version.
- `ReleaseUnit { version, source_revision, images: BTreeMap<String,
  ResolvedImage> }`; `ResolvedImage { repository, digest }`.
- `History` (from `discovery::history`) — the rollback-candidates listing
  type; bounded at 5 (`EXAMINED`), reports `more: bool`.
- `Decision` — `Advance(Release)` | `Stay(Reason)`.
- `Reason` — `Manual`, `Locked`, `Held`, `NothingNewer`,
  `UndefinedStablePolicy`.
- `decide(policy: UpdatePolicy, channel: Channel, held: bool, discovery:
  &Discovery) -> Decision` — pure, five branches, no I/O.
- `PlatformManagement` — `new(registry: Arc<dyn Registry>, charts: Arc<dyn
  ChartIndex>, desired_state: Arc<dyn DesiredState>, clock: Arc<dyn Clock>)
  -> Self`. `#[must_use] fn with_observer(mut self, observer: Arc<dyn
  DeploymentObserver>) -> Self` — builder-style; the observer field starts
  `None` and is only ever set this way. `async fn status/statuses`
  (read-only; when an observer is attached, also calls it and fills in
  `running`/`observation`). `async fn reconcile(environment, component) ->
  Reconciliation` (writes on `Decision::Advance`; never consults the
  observer). `async fn pause/resume(environment, component, note:
  Option<&str>) -> ComponentStatus`. `async fn rollback_candidates(environment,
  component) -> History`. `async fn roll_back(environment, component,
  version: &str, note: Option<&str>) -> ComponentStatus`. `async fn
  sweep(environment, state: &SweepState) -> SweepResult`.
- `PlatformError` — `DesiredState(#[from] DesiredStateError)`,
  `Registry(#[from] RegistryError)`, `NotAdvancing { component }`,
  `NotRollable { component, version }`.
- `ComponentStatus { component, desired: Version, newer: Option<Version>,
  running: Running, observation: Option<DeploymentObservation>, policy,
  artifact: ArtifactKind, hold, desired_state: DesiredStateStatus,
  diagnostics: Diagnostics }`. `.is_paused() -> bool` (`policy == Automatic
  && hold.is_some()`, rendered as `Automatic — Paused`).
  `ComponentStatus::assemble` (crate-internal) always sets `running:
  Running::Unknown, observation: None` — only `PlatformManagement::status`
  overwrites both, after calling the observer.
- `Running` — `Unknown` (no single healthy running version is currently
  established — the only value before PR #72, and still the value with no
  observer attached, no observation configured for the component, or a
  disagreeing/unhealthy observation) | `Observed(String)` (every active
  observed workload is healthy and agrees on this version — added by PR #72
  / ADR 0022).
- `DesiredStateStatus` — `Current` | `UpdateAvailable`.
- `Diagnostics { not_yet: Vec<Version>, incoherent: Vec<Version> }`.
- `Reconciliation { was: Version, status: ComponentStatus }`. `.advanced()
  -> bool` (`was != status.desired`).
- `Sweep { components: Vec<(String, Swept)> }` (`Default`).
- `Swept` — `Advanced { from, to }` | `Unchanged` | `Failed(PlatformError)`.
- `SweepResult` — `Ran(Sweep)` | `AlreadyRunning` | `NotConnected`.
- `SweepState` — holds an `AtomicBool` (`running`) and the last-check record.
  `record(...)`, exposed via `LastCheck`/`CheckOutcome`.
- `LastCheck` / `CheckOutcome` — the persisted "what did the last sweep
  find" record.
- `SafeDiagnostic` — `sanitise(text: &str) -> Self` (redacts credential
  prefixes, caps at 200 chars), `as_str()`, `Display`. The only constructor.

## Internal modules

- `artifact.rs` — `ArtifactSource`, `ArtifactKind`, `Release`.
- `version.rs` + `version/{ordering,parse}.rs` — `Version`, `Channel`; two
  grammars (`parse` for OCI tags, `parse_chart` for Helm chart versions,
  which may carry build metadata).
- `binding.rs` + `binding/{bound,delegate,generation,holding,live,swap}.rs`
  — `PlatformDesiredState`. `bound.rs`: `Bound` enum. `live.rs`: `Live`
  (the `Bound` + generation counter under one `RwLock`). `swap.rs`:
  `connect`/`unusable`/`disconnect`, all via a private `set` that bumps the
  generation. `generation.rs`: `tag`/`untag` a `DesiredRevision` with the
  binding generation it was read through — a mismatch is `Conflict`, not a
  refusal. `holding.rs`: `held()`/`writing()` produce an *owned* read guard;
  `outliving(guard, future)` spawns the delegated call in its own task so a
  dropped caller cancels nothing. `delegate.rs`: the actual
  `impl DesiredState for PlatformDesiredState`, tagging/untagging revisions
  at the boundary.
- `charts.rs` — `ChartIndex` trait alone.
- `desired_state.rs` + `desired_state/{component,errors,port}.rs` —
  `ComponentDesired`, `DesiredRevision`, `Hold`, `DesiredStateError`,
  `DesiredState` trait.
- `diagnostic.rs` + `diagnostic/redaction.rs` — `SafeDiagnostic`,
  `CREDENTIAL_PREFIXES` (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`,
  `github_pat_`, `hvs.`, `hvb.`), `MAX = 200`.
- `discovery.rs` + `discovery/{chart_history,chart_resolve,charts,history,unit}.rs`
  — forward search (above a floor) and history search (below, bounded at 5)
  for both artifact kinds; `unit.rs` is the OCI-specific
  candidate-then-assemble machinery (`Assembly::{Complete,Incomplete,Incoherent}`).
  A chart's `discover_chart`/`chart_history` never populate `not_yet` or
  `incoherent` — a chart is one artifact, so it cannot be half-published or
  disagree with itself the way several images can.
- `observation.rs` (ADR 0022) — `DeploymentHealth`, `WorkloadObservation`,
  `DeploymentObservation`, `DeploymentObserver`. The whole module is a data
  contract plus one trait; no logic beyond the type definitions.
- `policy.rs` — `UpdatePolicy` alone.
- `registry.rs` — `Registry` trait, `Resolved`, `Provenance`, `RegistryError`.
- `selector.rs` + `selector/selector_tests.rs` — `decide`, `Decision`,
  `Reason`. Pure.
- `service.rs` + `service/{backwards,brake,errors,look,reconcile,rollback}.rs`
  — `PlatformManagement` itself, including the `observer:
  Option<Arc<dyn DeploymentObserver>>` field and `with_observer`. `look.rs`:
  shared read+discover step behind `status`/`reconcile` (also owns
  `series_of`, the preview-only-series rule). `brake.rs`: `pause`/`resume`,
  the `PAUSED` hold reason, `stamp()` (RFC 3339 `since`). `backwards.rs`:
  rollback candidate/resolve dispatch by artifact kind, mirroring `look.rs`'s
  shape. `rollback.rs`: `rollback_candidates`/`roll_back`, the `ROLLBACK`
  hold reason. Only `status`/`statuses` (directly in `service.rs`) ever call
  the observer; `reconcile`, `pause`, `resume`, `roll_back` all build their
  `ComponentStatus` via `ComponentStatus::assemble`, which leaves
  `running`/`observation` at their zero values.
- `status.rs` + `status/reconciliation.rs` — `ComponentStatus` (with its
  `running`/`observation` fields) and friends, `Reconciliation`.
- `sweep.rs` + `sweep/{record,types}.rs` — `sweep`/`sweep_once`,
  `SweepState`, `SweepResult`, `Sweep`, `Swept`, `LastCheck`, `CheckOutcome`.
  `sweep_once` calls `reconcile`, never `status`, so a sweep never touches
  the observer either.

## Hard invariants — do not break

1. **`decide` cannot express clearing a hold or widening a policy.** It
   returns a `Version` (via `Release`), structurally incapable of carrying
   either — the guarantee is in the type, not a check somebody could forget.
2. **Discovery only ever considers versions strictly above (forward) or
   strictly below (rollback) a floor.** This is what makes "never move an
   environment backwards automatically" and "rollback only offers older
   versions" structural rather than asserted.
3. **A `PlatformDesiredState` write is refused as `Conflict` if its
   `DesiredRevision`'s generation tag does not match the current binding's
   generation**, in addition to the adapter's own staleness check on the
   untagged revision. Two independent conflict sources, both real.
4. **An unbind (`connect`/`unusable`/`disconnect`) waits for every operation
   already running against the previous binding**, and nothing a caller does
   (dropping the awaiting future) can shorten that wait — `outliving` moves
   the guard into a spawned task specifically so a cancelled caller cancels
   nothing real.
5. **`status`/`statuses` never write.** Only `reconcile`,
   `pause`/`resume`/`roll_back` do, and each states its precondition
   (`DesiredRevision`) explicitly.
6. **A rollback resolves the named version at write time**, never from a
   previously-fetched candidate list, and travels with a `Hold` in the same
   commit as the version change.
7. **`SafeDiagnostic::sanitise` is the only way an upstream error reaches a
   console.** It is not a substitute for adapters classifying their own
   errors, and it says nothing about what may reach a log line.
8. **This crate performs no I/O, holds no credential, and is transport-free** —
   it composes `Arc<dyn Registry>`, `Arc<dyn ChartIndex>`, `Arc<dyn
   DesiredState>` and (optionally) `Arc<dyn DeploymentObserver>`, and
   decides; it never constructs an adapter.
9. **`DeploymentObserver` is read-only and never authors desired state.**
   Nothing in `reconcile`, `advance`, `roll_back`, `pause` or `resume` reads
   the observer, and nothing here infers a running version from desired
   state having changed — a running version is only ever evidence handed in
   by whatever implements the port (ADR 0022).

## Notes

- `PlatformDesiredState`'s coordination (drain, generation, ordering) is
  **process-local**. There is no leader election and no cross-replica
  coordination; two control-plane processes racing a write are resolved by
  the adapter's own staleness check, independently in each process.
- `Cargo.toml`'s own comment on the `tokio` dependency spells out the
  intended discipline (task-owning `RwLock` guards, tasks that survive a
  cancelled caller, nothing else touching the runtime) — `rt` is there only
  because `tokio::spawn` needs a runtime to spawn into, not because this
  crate starts one.
- `service_tests.rs`, `selector_tests.rs`, `discovery_tests.rs`,
  `version_tests.rs`, `binding_tests.rs`, `sweep_tests.rs` and
  `diagnostic_tests.rs` are all inline `#[cfg(test)]` modules beside the
  code they test, per house convention — there is no separate `tests/`
  integration suite in this crate; adapters (`fabric-platform-git`,
  `fabric-registry`, `fabric-deployment-kubernetes`) carry the
  integration-level proof against real or fake transports.
- `fabric-deployment-kubernetes` is the shipped `DeploymentObserver`
  implementation; it reaches Kubernetes over plain HTTPS with a projected
  service-account token, never a `kube`-family client
  (`scripts/check_architecture.py`'s `CONTROL_PLANE_CLIENTS` bans one
  workspace-wide). See its own `docs/` and ADR 0022 for what evidence counts
  as a healthy running version.
