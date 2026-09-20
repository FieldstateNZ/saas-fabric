# fabric-platform-management — LLM context

Decides which version of a platform component an environment should run:
channels, update policy, discovery, rollback, the late-bound connection to
the currently-live platform repository, and (since PR #72, ADR 0022) an
optional read-only bridge to independently observed deployment evidence.
Since ADR 0023 part 1 it also decides what an environment declares about its
data sources — a second declared resource, `data_sources.rs`, over the same
late-bound repository. Since ADR 0023 part 2 it also places a client's data
intent on one of those declared sources and records the outcome —
`placements.rs`, a third module over the same repository. Since ADR 0023
part 4 it also composes what is declared and recorded, plus a derived
runtime catalogue read through a seam this crate defines, into the runtime's
three documents and offers them to a publication target on demand —
`publication.rs`, a fourth module, the one place this crate touches a port
outside `DesiredState`/`DataSourceState`/`PlacementState`. In neither plane (see
`docs/architecture/crate-dependencies.md`) — no transport, no HTTP, no Git,
no Kubernetes client. Depends on `fabric-core` and, since ADR 0023,
`fabric-runtime-publication` (also in neither plane — reused for the wire's
own data-source sub-types rather than re-declared, so a hand-editable
`data-sources.yaml` cannot disagree with what gets published from it), plus
`async-trait`, `serde`, `thiserror`, `time`, `tokio` (declares `sync` and
`rt`; links the workspace's additive `macros`, `rt-multi-thread`, `sync`,
`time` set, as its Cargo.toml says), `tracing`.

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
  PlatformRepository>)`, `async fn unusable(&self, detail: &str)`, `async fn
  disconnect(&self)` — all wait for every in-flight operation against the
  *previous* binding (drain). Implements `DesiredState`, `DataSourceState`,
  `PlacementState` and `PlatformRepository` itself (the last by delegation
  in `binding/environment.rs`, untagging both halves' revisions), so it is
  interchangeable with any port it wraps — see `PlatformRepository` below.
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
  `NotRollable { component, version }`, `InvalidDataSource(#[from]
  DataSourceRule)` (ADR 0023 part 1 — kept apart from `DesiredState` because
  it is refused on its own terms, before anything is read or written),
  `InvalidHeldDataSources { detail }` (a hand-edited `data-sources.yaml`
  `check_held` refused), `PlacementRefused(#[from] PlacementRefusal)` (ADR
  0023 part 2 — `select` refused the intent), `InvalidHeldPlacements
  { detail }` (`InvalidHeldDataSources`'s sibling, from
  `check_held_placements`), `DataSourceInUse { id: DataSourceId, tenants:
  Vec<TenantId> }` (a data source `DataSources::remove` refused to drop
  because a placement still names it).
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
- `DataSourceDeclaration { id: DataSourceId, revision: BindingRevision,
  connector: ConnectorId, connection: ConnectionSelectorDocument, placement:
  PlacementClassDocument, residency: DataResidencyDocument, pool:
  PoolSettingsDocument, capabilities: DataSourceCapabilitiesDocument,
  discriminator: Option<Discriminator>, labels: BTreeMap<String, String> }`
  (`Serialize`/`Deserialize`, `deny_unknown_fields`) — ADR 0023 part 1.
  `into_document(self) -> DataSourceDocument` drops `discriminator`
  field-by-field; `validate(&self) -> Result<(), DataSourceRule>` is the
  whole rule set below.
- `Discriminator { column: FieldName }` — the one field the wire's
  `DataSourceDocument` does not carry.
- `ConnectorId`, `ConnectionSelectorDocument`, `PlacementClassDocument`,
  `DataResidencyDocument`, `PoolSettingsDocument`,
  `DataSourceCapabilitiesDocument`, `DataSourceDocument`, `FieldName`,
  `ConnectionName` — re-exported straight from `fabric_runtime_publication`
  at this crate's root, not redeclared, so a caller that builds or reads a
  declaration names the wire's own types without a second Cargo dependency.
- `DataSourceRule` — `SharedNeedsDiscriminator` | `DiscriminatorOnlyWhenShared
  { placement: PlacementClassDocument }` | `ZeroPool { field: PoolField }` |
  `EmptyLabel` | `MalformedSecretReference` | `ConnectionKindNotDeclarable`
  (the wire's `Default {}` connection shape — an operator names a connection
  or a secret, never leaves it unstated). Fields are the wire's own typed
  enum or `PoolField`, not a pre-rendered word, so the platform's phrasing
  lives in one place, the `Display` impl. `PoolField` — `MaxConnections` |
  `IdleTimeoutSeconds` | `AcquireTimeoutSeconds`, naming which pool setting
  was zero. Each `DataSourceRule` variant's `Display` is the message an
  operator reads; none names a file.
- `DataSourcesRead { revision: Option<DesiredRevision>, declarations:
  Vec<DataSourceDeclaration> }` — declarations sorted by id; `revision` is
  `Option` at the port because an adapter's truth about a file is genuinely
  optional there (`None` when no file exists yet for the environment asked
  about — per document, not per id). `PlatformDesiredState` always fills it
  with `Some` before a caller above the binding ever sees it, tagging even
  an absent file with a generation.
- `DataSourceState` (async trait) — `read_data_sources(environment) ->
  DataSourcesRead`; `write_data_sources(environment, declarations: &[…], at:
  Option<&DesiredRevision>, message) -> Result<(), DesiredStateError>` — a
  whole-document compare-and-swap replace, `at: None` meaning "create; refuse
  if present", per document (the whole environment's data sources), never per
  id. Not a supertrait of `DesiredState` — see `PlatformRepository`.
- `PlatformRepository` (`binding/repository.rs`) — `trait PlatformRepository:
  DesiredState + DataSourceState + PlacementState { async fn
  write_environment(&self, environment: &str, write: EnvironmentWrite<'_>,
  message: &str) -> Result<(), DesiredStateError>; }`. **Not a pure blanket
  trait any more** (ADR 0023 part 2, B4): `write_environment` writes the
  data-sources and placements documents in one atomic commit, and a generic
  implementation over the three independent ports cannot know how to do
  that, so every connectable type must name this trait and supply its own
  atomic write — `PlatformGitRepository` (`fabric-platform-git`,
  `src/port/environment.rs`) over `update_files_atomically` with two
  `FileChange`s, the late-bound binding (`binding/environment.rs`) by
  delegation untagging both revisions against one generation, and every
  test fixture that connects to a binding needs its own implementation too.
  What `PlatformDesiredState::connect` accepts: one connected repository
  answers every port, because `environments/ENV/placements.yaml` lives
  beside `data-sources.yaml` and `components.yaml` in the same repository,
  written by the same credential (ADR 0023 parts 1 and 2).
- `EnvironmentWrite<'a> { data_sources: (&'a [DataSourceDeclaration],
  Option<&'a DesiredRevision>), placements: (&'a [PlacementRecord],
  Option<&'a DesiredRevision>) }` (`binding/repository.rs`) — what
  `write_environment` takes: both documents' complete lists, each with the
  revision it was read at. The unchanged half is re-rendered from the list
  the caller read even when nothing about it changed; for a file only
  Fabric has written that reproduces the same bytes, so a content-addressed
  adapter gives back the same revision and the sibling does not move, but
  for a break-glass file with a comment *inside* the entry list (not the
  preserved header) the comment is dropped on re-render, the bytes differ,
  and the sibling's revision moves even though nothing about its meaning
  did.
- `DataSources { state: Arc<dyn DataSourceState> }` — `new(state)`. `async fn
  list(environment) -> DataSourcesRead` (read-only). `async fn
  declare(environment, declaration, at: Option<&DesiredRevision>) ->
  Result<Declared, PlatformError>` — validates first, then computes the
  revision itself (1 for a new id, held + 1 on any other field changing,
  unchanged when nothing does), and calls the port at all only when
  something is actually being written; uses `write_data_sources` alone, a
  single-document compare-and-swap. `async fn remove(environment, id, at:
  Option<&DesiredRevision>, repository: &dyn PlatformRepository) ->
  Result<Declared, PlatformError>` (ADR 0023 part 2) — reads both
  documents through `repository` (not `self.state`: this is the one
  operation on this service that needs every port, to call
  `write_environment` rather than `write_data_sources` alone), refuses
  `DataSourceInUse` naming every tenant still placed on `id` (deduplicated
  — one tenant with two logical placements on the source being removed is
  named once), otherwise writes both documents in one commit, the data
  sources without `id` and the placements list unchanged. `repository` is
  taken per call, not at construction, so the check and the write see the
  same state; `DataSources::new`'s own constructor is unchanged by ADR 0023
  part 2 (`data_sources/service/remove.rs`).
- `Declared` — `Written(DataSourcesRead)` | `Unchanged(DataSourcesRead)`, so a
  caller can tell "nothing changed" from "this is what changed to" without
  comparing documents itself.
- `DataIntent { class: PlacementClassDocument, provider: Option<String>,
  region: Option<String> }` (ADR 0023 part 2) — a structural twin of
  `fabric_client_model::DataIntent`, declared again here rather than
  depended on (neither crate may depend on the other; `fabric-control-plane`
  is the one caller with both types in scope and converts between them).
- `PlacementRecord { tenant: TenantId, logical: LogicalDataSourceName,
  data_source: DataSourceId, isolation: IsolationModelDocument, placed_at:
  String }` (`Serialize`/`Deserialize`, `deny_unknown_fields`) — the fact,
  once `select` has decided it. `isolation` is the wire's own
  `IsolationModelDocument`, never a second declaration of its three shapes.
- `IsolationModelDocument` — re-exported from `fabric_runtime_publication`
  at this crate's root, same reason as the data-source sub-types:
  `Database {}` | `Schema { schema: SchemaName }` | `Discriminator { column:
  FieldName, value: String }`.
- `PlacementsRead { revision: Option<DesiredRevision>, placements:
  Vec<PlacementRecord> }` — placements sorted by (tenant, logical);
  `revision` is `Option` at the port for the same reason `DataSourcesRead`'s
  is, and the binding always fills it with `Some`.
- `PlacementState` (async trait) — `read_placements(environment) ->
  PlacementsRead`; `write_placements(environment, placements: &[…], at:
  Option<&DesiredRevision>, message) -> Result<(), DesiredStateError>` — the
  same whole-document compare-and-swap shape `DataSourceState` is. Not a
  supertrait of `DesiredState` — see `PlatformRepository`.
- `select(intent: &DataIntent, tenant: &TenantId, logical:
  &LogicalDataSourceName, declared: &[DataSourceDeclaration], held:
  &[PlacementRecord], now: &str) -> Result<PlacementRecord,
  PlacementRefusal>` (`placements/select.rs`, `placements/select/pick.rs`)
  — pure, the only place ADR 0023 part 2 lets this decision be made. Order:
  `AlreadyPlaced` if `held` already has (tenant, logical); candidates are
  `declared` entries matching `intent.class`, `accepts_new_tenants`,
  `writable`, and `residency.region` when `intent.region` is stated
  (`provider` never matched); on `Shared`, the candidate with the fewest
  held placements wins (ties by lowest id), isolated by `Discriminator`
  naming the tenant id as `value` — a collision is checked against
  *different* tenants only, since the value is always this tenant's own id
  and a second logical placement of its own on the same source legitimately
  repeats it (B2) — refusing `DiscriminatorValueTaken` if a different
  tenant already holds that value; on any other class, the lowest-id
  candidate with **no** held placement wins, isolated by `Database {}`
  (`Schema` is never produced — ADR 0006 calls it inert); no candidate at
  either step is `NoDataSourceAdmits { class, region, provider }` when
  nothing declared even matched the class/region/capabilities, or
  `AllMatchingSourcesOccupied { class }` when something did but — because
  the class is not `Shared` — every one that matched already holds a
  tenant (N2: kept apart from `NoDataSourceAdmits` so an operator is never
  sent to declare a data source they already declared).
- `PlacementRefusal` (`thiserror`) — `AlreadyPlaced { tenant, logical }` |
  `NoDataSourceAdmits { class, region, provider }` |
  `AllMatchingSourcesOccupied { class }` | `DiscriminatorValueTaken {
  data_source, value }` | `TenantIdInvalid { client }` (the client id
  `Placements::place`/`for_client` reparse as a `TenantId` is not one,
  N11). Each variant's `Display` is the operator-facing message; none names
  a file.
- `PlacementOutcome` — `Placed(PlacementRecord)` | `Placeable` |
  `Refused(PlacementRefusal)`. Three states, not two: "nothing recorded, and
  `select` would place it" and "nothing recorded, and `select` would refuse
  it" are different facts a caller acts on differently.
- `ClientPlacements { revision: Option<DesiredRevision>, entries:
  BTreeMap<LogicalDataSourceName, PlacementOutcome> }` — what
  `Placements::for_client` found, one entry per logical data source asked
  about.
- `Placements { repository: Arc<dyn PlatformRepository>, clock: Arc<dyn
  Clock> }` — `new(repository, clock)`. One field for every port, not two
  separate `Arc`s: `place` needs `write_environment`, which only exists on
  the combined trait, and both operations' own signatures are fixed by ADR
  0023 part 2 with no room for an extra parameter to carry it in instead.
  `async fn for_client(environment, client: &str, intents:
  &BTreeMap<LogicalDataSourceName, DataIntent>) -> Result<ClientPlacements,
  PlatformError>` — reparses `client` as a `TenantId` itself (N11, refusing
  `PlacementRefusal::TenantIdInvalid` if it is not one, so the control
  plane never does this conversion), reads both held documents (validating
  each), runs `select` per intent without writing, and returns the
  preview. `async fn place(environment, client: &str, logical, intent, at:
  Option<&DesiredRevision>) -> Result<PlacementsRead, PlatformError>` —
  reparses `client` the same way, checks `at` against the held placements
  revision *immediately after reading it* and before data sources are even
  read (N9; same precondition-before-planning order as
  `DataSources::declare`), computes `now` via a fallible `stamp()` that
  refuses `DesiredStateError::Unavailable` rather than record an empty
  `placed_at` when the clock cannot be formatted as RFC 3339 (N4), then
  calls `write_environment` with the data sources it read (unchanged) and
  the placements list with `select`'s new record appended. Split across
  `service.rs` (the struct, `place`), `service/for_client.rs` (`for_client`),
  and `service/stamp.rs` (`stamp`); `tenant_id.rs` holds the shared
  reparse, since both operations need it.
- `compose(declarations: Vec<DataSourceDeclaration>, placements:
  &[PlacementRecord], catalog: CatalogDocument, held: &PublishedRevisions)
  -> Result<RuntimeSnapshot, ComposeError>` (ADR 0023 part 4, D2/D3) — pure.
  Data sources become `DataSourceDocument`s, sorted by id. Placements group
  by tenant into one `TenantBindingDocument` each, `revision` the *sum* of
  the tenant's records' own (D1). Every document offered at `held`'s own
  revision, or `1` when none. Never sets emptying intent.
  `ComposeError::DuplicatePlacement { tenant, logical }` is the one refusal.
- `RuntimeCatalogueSource` (async trait) — `async fn runtime_catalogue(&self)
  -> Result<CatalogDocument, CatalogueSourceError>`. The seam that keeps this
  crate off `fabric-client-model`; the control plane implements it.
- `CatalogueSourceError` — `Conflict { resource, applications: (String,
  String) }` | `Unavailable(String)`.
- `RuntimePublisher { environment, platform: Arc<dyn PlatformRepository>,
  catalogue: Arc<dyn RuntimeCatalogueSource>, target: Arc<dyn
  RuntimePublication>, clock: Arc<dyn Clock> }` — `new(...)`,
  `describe_target() -> String`, `async fn publish_once(&self, state:
  &PublicationState) -> PassResult`. Offers at the held revision; on
  `DivergentPayload`, bumps *only* the named document and re-offers, up to
  three times (one per document, `protocol.rs::MAX_RETRIES`). Guarded
  against re-entry by `PublicationState`'s atomic flag, released by a
  `RunningGuard` (`Drop`-based, `running_guard.rs`) taken right after the
  swap -- released on a panic and on cancellation (a dropped future) as well
  as a normal return, which a post-`.await` `store(false, ...)` would not
  be.
- `PublicationState` — `SweepState`'s sibling: `new()`, `last_pass() ->
  Option<LastPass>`.
- `LastPass { at_unix_seconds: u64, outcome: PassOutcome }`.
- `PassOutcome` — `Published { tenants, data_sources, catalog:
  DocumentOutcome each, revisions: PublishedRevisions }` | `Unchanged {
  revisions }` | `Waiting { reason: WaitingReason }` | `Refused { reason:
  SafeDiagnostic }` | `Failed { detail: SafeDiagnostic }`.
- `WaitingReason` — `NoResources` (the derived catalogue has none yet; ADR
  0018's "create empty documents at startup" is superseded, see that ADR's
  amendment) | `PlatformNotConnected` (no operator has connected this
  environment's platform repository yet -- `SweepResult::NotConnected`'s
  sibling, not a failure).
- `PassResult` — `Ran { at_unix_seconds: u64, outcome: PassOutcome }` |
  `AlreadyRunning`. `at_unix_seconds` is the same moment `PublicationState`
  just recorded the outcome at -- the one clock `publish_once` reads, so a
  caller rendering a response from this never mints a second, later
  timestamp `GET /api/platform` would then disagree with for the same pass.

## Internal modules

- `artifact.rs` — `ArtifactSource`, `ArtifactKind`, `Release`.
- `version.rs` + `version/{ordering,parse}.rs` — `Version`, `Channel`; two
  grammars (`parse` for OCI tags, `parse_chart` for Helm chart versions,
  which may carry build metadata).
- `binding.rs` +
  `binding/{bound,data_sources,delegate,environment,generation,holding,live,placements,repository,swap}.rs`
  — `PlatformDesiredState`. `bound.rs`: `Bound` enum, holding
  `Arc<dyn PlatformRepository>` once connected. `repository.rs`:
  `PlatformRepository` trait (no supertrait relationship among
  `DesiredState`/`DataSourceState`/`PlacementState`, on purpose -- see its
  own rustdoc) plus `EnvironmentWrite`; no blanket impl any more, since
  `write_environment` needs a real one (ADR 0023 part 2, B4). `live.rs`:
  `Live` (the `Bound` + generation counter under one `RwLock`), and
  `repository()`/`data_source_repository()`/`placement_repository()`/
  `platform_repository()`, which upcast the stored `Arc<dyn
  PlatformRepository>` to each port in turn (`platform_repository()`
  hands back the whole trait, unupcast, for `write_environment`). `swap.rs`:
  `connect`/`unusable`/`disconnect`, all via a private `set` that bumps the
  generation; `connect` takes `Arc<dyn PlatformRepository>`. `generation.rs`:
  `tag`/`untag` a `DesiredRevision` with the binding generation it was read
  through — a mismatch is `Conflict`, not a refusal — plus
  `tag_presence`/`untag_presence`, the same idea for a data-sources read
  whose file may not exist: the *absence* of a file is generation-tagged
  too, so a create decided through this binding is exactly as generation-safe
  as a replace. `holding.rs`: `held()`/`writing()` produce an *owned* read
  guard; `outliving(guard, future)` spawns the delegated call in its own
  task so a dropped caller cancels nothing. `delegate.rs`: the actual
  `impl DesiredState for PlatformDesiredState`, tagging/untagging revisions
  at the boundary. `data_sources.rs`: the same shape as `delegate.rs`, for
  `impl DataSourceState for PlatformDesiredState` — `read_data_sources`
  always returns `Some` revision, even for an absent file, via
  `tag_presence`. `placements.rs`: the same shape again, for `impl
  PlacementState for PlatformDesiredState`. `environment.rs`: `impl
  PlatformRepository for PlatformDesiredState` — `write_environment`
  untags *both* halves' revisions against the one generation read under
  the one guard, so a rebind between the read and the write refuses both
  at once rather than leaving one checked and the other trusted.
- `charts.rs` — `ChartIndex` trait alone.
- `data_sources.rs` +
  `data_sources/{declaration,held,plan,port,read,rule,service,validate}.rs`
  + `data_sources/service/remove.rs`
  (ADR 0023 part 1, part 2 for `remove`) — `declaration.rs`:
  `DataSourceDeclaration`, `Discriminator`, `into_document`. `validate.rs`:
  `DataSourceDeclaration::validate`, the `DataSourceRule` checks. `rule.rs`:
  `DataSourceRule`, `PoolField`. `held.rs` (`pub(crate) mod`, crate-wide so
  `placements::service` and `data_sources::service::remove` can both reach
  it): `check_held(declarations) -> Result<(), DesiredStateError>` —
  refuses a held document a hand edit made incoherent (two entries with one
  id, or an entry that no longer validates); called by `list`, `declare`
  and `remove` on *every* read, not only on write, since a break-glass edit
  can land between any two calls. `port.rs`: `DataSourceState` trait.
  `read.rs`: `DataSourcesRead`. `service.rs`: `DataSources`, `Declared`
  (its own docs now say what `remove` does too, not only `declare`).
  `service/remove.rs`: `DataSources::remove`, split out once the method
  needed `write_environment` and grew past this crate's line budget.
  `plan.rs` (`pub(crate)`): `plan(held, incoming) -> Plan` — the pure
  revision/no-op decision behind `declare`, tested on its own in
  `plan_tests.rs`.
- `placements.rs` +
  `placements/{held,intent,outcome,port,read,record,refusal,select,service,tenant_id}.rs`
  + `placements/held/rules.rs` + `placements/select/pick.rs` +
  `placements/service/{for_client,stamp}.rs`
  (ADR 0023 part 2) — `intent.rs`: `DataIntent`. `record.rs`:
  `PlacementRecord`. `refusal.rs`: `PlacementRefusal`, its `Display`
  messages, and the one-line reason this file sits in the 121-150 line band
  (one enum, one message-building helper per non-trivial variant, and
  splitting either from the other would separate a refusal from the words
  an operator reads for it). `outcome.rs`: `PlacementOutcome`,
  `ClientPlacements`. `port.rs`: `PlacementState` trait. `read.rs`:
  `PlacementsRead`. `held.rs` (`pub(crate) mod`): `check_held_placements(placements,
  declared) -> Result<(), PlatformError>` — `data_sources::held::check_held`'s
  sibling, called by `for_client`, `place` and `DataSources::remove` on
  every read; delegates the isolation/class match to `held/rules.rs`
  (`isolation_matches_class`), split out to keep this file under the line
  budget. `select.rs` + `select/pick.rs`: `select`, the pure selector, and
  its rules 3-5 (`pick_shared`, `pick_exclusive`). `tenant_id.rs`: the
  client-id-to-`TenantId` reparse shared by `place` and `for_client` (N11).
  `service.rs` + `service/for_client.rs` + `service/stamp.rs`: `Placements`,
  `for_client` (preview, no write), `place` (write, via
  `write_environment`), `stamp` (the fallible RFC 3339 clock read, N4).
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
- `publication.rs` +
  `publication/{catalogue_source,compose_error,outcome,pass,pass_tests,protocol,protocol_tests,publish_error,publisher,publisher_tests,reads,reads_tests,running_guard,snapshot,snapshot_tests,state,testing}.rs`
  (ADR 0023 part 4) — `compose` (`snapshot.rs`, pure: declared data sources +
  recorded placements + derived catalogue + held revisions -> `RuntimeSnapshot`),
  `ComposeError::DuplicatePlacement` | `EmptyTenantBinding` (`compose_error.rs`,
  split out of `snapshot.rs` once the enum's own rustdoc outgrew sharing a file
  with the computation that raises it), `RuntimeCatalogueSource` +
  `CatalogueSourceError` (`catalogue_source.rs`, the seam that keeps this
  crate off `fabric-client-model`), `RuntimePublisher` (`publisher.rs`:
  `new`, `describe_target`, `publish_once`, the one thing here that touches
  a port), the offer-and-advance retry rule (`protocol.rs`, `MAX_RETRIES =
  3`), `PublicationState` + `LastPass` (`state.rs`, `SweepState`'s
  sibling), `PassOutcome` + `WaitingReason` + `PassResult` (`outcome.rs`,
  `WaitingReason::{NoResources, PlatformNotConnected}`).
  `running_guard.rs`: `RunningGuard`, the `Drop`-released re-entry guard
  `publish_once` takes immediately after winning the atomic swap -- released
  on a panic and on cancellation, not only a normal return. `pass.rs`:
  `run_pass`, the body `publish_once` runs once that guard lets it through --
  including the `current()` read at its own top, routed through the same
  classifier as every other adapter error rather than a blanket `Failed`.
  `publish_error.rs`: `outcome_from_publish_error`, which sorts an adapter's
  `PublicationError` into `Failed` (`Unwritable`, `Unreadable`,
  `StaleRevision` -- transport problems a retry or the next read fixes) or
  `Refused` (everything else, including an exhausted `DivergentPayload`
  budget -- a document a human must fix). `reads.rs`: reading the three
  inputs, turning `DesiredStateError::Refused` and a coherence problem into
  `Refused`, `NotConnected` into `Waiting { PlatformNotConnected }`, and
  `Unavailable`/`Conflict` into `Failed`. `testing.rs` (`#[cfg(test)]`): an
  in-memory `RuntimePublication` fake for
  `protocol_tests.rs`/`publisher_tests.rs`, with a `publish_attempts()`
  counter that pins the retry budget from above. The control plane's own half —
  its `RuntimeCatalogueSource` impl (`fabric-control-plane`'s
  `service/runtime_catalogue_source.rs`), the `/api/platform` row, the
  operator trigger, and the schedule (`fabric-control-plane-api`'s
  `startup/platform/{publication,publishing}.rs`) — is not here.
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
10. **`select` is the only place a placement is decided, and it never runs
    again for a tenant already placed.** `Placements::place` and
    `Placements::for_client` both check `held` for the (tenant, logical)
    pair before calling `select`; publication reads `PlacementRecord` and
    copies it, it never recomputes one — the record is the fact, not a
    formula run again (ADR 0007, ADR 0023 part 2).

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
