# fabric-platform-management

Decides which version of a platform component an environment *should* run:
channels, update policy, discovery, rollback, and the late-bound connection
to whichever platform repository is currently live. It also holds the
optional, read-only bridge to independently observed deployment evidence.
This is the domain crate for the Platform Management capability described in
[`docs/architecture/control-plane.md`](../../../docs/architecture/control-plane.md).

Sits in neither plane (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
Depends only on `fabric-core`; knows nothing about HTTP, a specific registry,
Git, or Kubernetes.

## Why this crate exists

The specification's model is three states — Desired, Available, Running —
and this crate exists because collapsing any pair of them produces a console
that reports success from a Git commit:

```text
Available (Newer version)   discovered from artifact registries  <- this crate asks
Desired                     the platform repository              <- this crate proposes
Running                     independent deployment evidence       <- this crate reports, never authors
```

A version that has been *published* is not one that has been *chosen*, and a
version that has been chosen is not one that is *serving traffic*. This
crate owns the first two questions outright. The third — what is actually
running — it can now *report*, but only as evidence handed to it by an
optional [`DeploymentObserver`](../../../docs/decisions/0022-running-versions-come-from-deployment-evidence.md)
port (ADR 0022): when nothing is attached, or the observer has nothing to
say, `Running` stays `Unknown`, exactly as it always did.

It knows nothing about *how* an artifact was found, *how* the platform
repository is reached, or *how* running evidence is gathered. It defines
three ports — `Registry` (image discovery), `ChartIndex` (chart discovery)
and `DeploymentObserver` (read-only running evidence) — plus `DesiredState`
(reading and moving desired state), and is handed implementations:
`fabric-registry` implements the first two, `fabric-deployment-kubernetes`
the third, and `fabric-platform-git` the fourth. Keeping the registry
credential, the platform repository's Git credential, and the Kubernetes
read-only identity three separate integrations is a large part of the point
(see `docs/architecture/crate-dependencies.md`).

## Key concepts

- **Two artifact kinds, not one shape with fields left empty**
  (`ArtifactSource`, `ArtifactKind`). `Oci { repositories }` — several
  container images that must all carry a version and agree on the commit
  they were built from; what deploys is an immutable digest. `Helm {
  repository, chart }` — a chart repository pins a *version*, not a digest,
  so the bytes behind a version number can be republished later. Both kinds
  can be rolled back; they differ in *how much* of the old release comes
  back, and that difference is stated to the operator (`ComponentStatus`
  carries the `ArtifactKind`, not a `rollable: bool`), never enforced by
  refusing one kind.
- **`Version`** — `SemVer` precedence, not string order (`preview.9` sorts
  before `preview.10`; a string comparison has that backwards). Equality is
  precedence, not spelling: two versions differing only in build metadata
  compare equal, and the chart index refuses such a collision rather than
  picking between them (see `charts.rs`' `ChartIndex::versions` docs).
- **`UpdatePolicy`** — `Automatic` (moves without asking) | `Manual`
  (surfaced, an operator chooses) | `Locked` (nothing moves without editing
  the constraint itself).
- **`decide` (`selector.rs`)** — the whole automatic-advancement rule, five
  branches, no arithmetic: `Manual`/`Locked` stay; `Automatic` with a hold
  stays (`Held`); `Automatic` on the `Stable` channel stays
  (`UndefinedStablePolicy` — deliberately: every stable release changes the
  `major.minor.patch` core, so the line-bounded rule that is safe for a
  preview would let a sweep take a `7.3.0 -> 8.0.0` jump at 3am, and nobody
  has decided that is allowed yet); `Automatic` otherwise advances to
  whatever `Discovery::newer` found, or stays with `NothingNewer`. It cannot
  clear a hold or widen a policy to succeed — the type it returns
  (`Decision::Advance(Release)`) carries only a version, structurally.
- **`Discovery` / `discover` / `history` / `resolve`** — a forward search
  only ever considers versions *strictly above* the current one (which is
  what makes automatic selection structurally unable to move an environment
  backwards), and every rejected candidate below the one selected is still
  reported (`not_yet` — still publishing, expected to resolve itself;
  `incoherent` — images that disagree about their source commit, which no
  amount of waiting fixes) so an environment that jumps `preview.2` to
  `preview.4` can say what happened to `preview.3`.
- **Rollback restores an older *published* version, not a history.** Nothing
  here remembers what an environment ever ran; `rollback_candidates` and
  `roll_back` search *backwards* from the desired version, within the same
  channel and (for a preview) the same line, against whatever a registry or
  chart repository publishes *now*. `roll_back` re-resolves the named
  version at write time rather than trusting a candidate object the caller
  is holding — a version withdrawn between the two requests is refused, not
  deployed from stale data — and always writes the version and a `Hold`
  together, in one commit: an environment moved backwards under a live
  `Automatic` policy would otherwise be advanced forward again by the very
  next sweep.
- **`PlatformDesiredState`** — the late-bound connection to whichever
  platform repository is currently connected. See "The platform binding" below.
- **`DeploymentObserver` (`observation.rs`, ADR 0022)** — an optional,
  read-only port: `observe(environment, component) -> Option<DeploymentObservation>`.
  `None` means this component has no observation binding configured, not
  that observation failed. When `PlatformManagement` is built `with_observer`,
  `status`/`statuses` call it and use the result to fill in `Running` and
  `ComponentStatus::observation` — nothing else in this crate ever calls it,
  and nothing here ever uses its answer to decide what to write. A running
  version is *evidence handed in*, never something this crate infers from
  desired state having changed.
- **`SafeDiagnostic`** — the only way to put upstream text in front of an
  operator. Redacts known credential prefixes (`ghp_`, `hvs.`, ...) and caps
  length at 200 characters. It is a tripwire, not a proof: the first line of
  defence is still adapters classifying their own failures rather than
  forwarding somebody else's words, and it is explicitly **not** a license to
  log the unredacted original — logs are a different, longer-lived, more
  widely-forwarded audience than a console line.

## The platform binding

`PlatformDesiredState` is late-bound because, per
[ADR 0011](../../../docs/decisions/0011-the-platform-creates-its-own-git-application.md),
the platform repository and its credential are not configuration — an
operator connects them from inside the product. At startup there is nothing
to build from, so a control plane that refused to start without one could
never be used to connect one.

Three states, not an `Option` (`Bound` in `binding/bound.rs`): `Nothing` (no
operator has connected a repository — a console renders "not connected");
`Repository(Arc<dyn PlatformRepository>)` (live — both ports, one adapter,
so `data-sources.yaml` and `components.yaml` are always answered by the same
connected repository); `Unusable(SafeDiagnostic)` (an operator connected one
and it does not work — reported as broken, not as "not connected", because
those lead an operator to different actions).

**An unbind drains.** Changing the binding (`connect`/`disconnect`/`unusable`)
takes the write lock, so it completes only once every operation that began
against the *old* repository has an outcome — and nothing starts against it
afterwards. This is process-local: it is one `tokio::sync::RwLock` inside one
process, and a second control-plane replica shares neither the lock nor the
generation counter below. Designing shared coordination across replicas is a
separate problem this crate does not solve.

**Every read is tagged with a generation** (`binding/generation.rs`) so that
a decision read through repository A, applied after an operator rebinds to
repository B, is a `Conflict` — "the state you decided against has moved" —
rather than a request B is free to interpret however it likes. A
generation mismatch is deliberately a `Conflict`, never a refusal: the
operator did nothing wrong by rebinding, and the caller's remedy (re-read,
decide again) is identical to an ordinary stale-revision conflict.

**Nothing a caller does can cancel the drain.** Every delegated operation
runs inside a `tokio::spawn`ed task that owns the read guard
(`binding/holding.rs::outliving`) — a caller whose future is dropped (a
request timeout, a closed browser) cancels nothing, because the task is not
the caller's future. The only two things that *do* leave an operation
unresolved are a panic inside it and process shutdown, and both are stated
plainly as residuals rather than hidden.

## How the pieces fit

```text
PlatformManagement            the domain service, over three ports (+ an optional fourth)
  |         |          |               |
  registry  charts      desired_state   observer (optional)
  (images)  (charts)    (read + move)   (running evidence)
       \        |            /               |
        \       |           /                |
   Registry  ChartIndex  DesiredState   DeploymentObserver   <- ports this crate defines
       |          |            |               |
  fabric-registry (both)   fabric-platform-git   fabric-deployment-kubernetes
```

`PlatformManagement::status`/`statuses` never write (a page load must not be
able to move an environment); when an observer is attached, they also call
it to fill in `Running` and `ComponentStatus::observation`. `reconcile`
reads, discovers, decides, and writes only on `Decision::Advance` — and
reports what it *did*, from the write, not from a second read that would
race the write it just performed; it never consults the observer, because
what a component advances *to* is decided from desired state and discovery
alone. `sweep` calls `reconcile` for every component of an environment, is
not a scheduler (something else decides cadence) and is not a distributed
lock (two control planes sweeping at once both decide independently; the
loser's write is refused as stale, which is wasteful and not dangerous).

## Data sources

[ADR 0023](../../../docs/decisions/0023-data-sources-are-environment-desired-state-and-placement-is-recorded.md)
part 1 adds a second declared resource beside components: what an environment
can place a tenant's data on. It lives in its own module, `data_sources.rs`,
and shares nothing with channels, discovery or rollback except the late-bound
repository connection.

- **`DataSourceDeclaration`** — one data source: the wire's own sub-types
  (`ConnectorId`, `ConnectionSelectorDocument`, `PlacementClassDocument`,
  `DataResidencyDocument`, `PoolSettingsDocument`,
  `DataSourceCapabilitiesDocument` — all re-exported from this crate's root,
  straight from `fabric_runtime_publication`, so a caller never has to depend
  on that crate directly) plus `Discriminator`, the one field the wire does
  not carry. `into_document` drops it on the way to the published shape;
  placement copies it into a tenant binding later (ADR 0023 part 2, not
  built).
- **`DataSourceRule`** — why a declaration is refused: a shared source with
  no discriminator column, a discriminator on anything else, a pool setting
  of zero (`ZeroPool { field: PoolField }`), a label with an empty key or
  value, a malformed secret reference, or the wire's third connection
  shape — `Default {}`, the connector's own single connection — which an
  operator can never declare (`ConnectionKindNotDeclarable`). Every field is
  the wire's own typed enum (`PlacementClassDocument`) or this crate's
  (`PoolField`), never a pre-rendered word, so the platform's phrasing lives
  in one place, the `Display` impl. Checked by
  `DataSourceDeclaration::validate` before anything is read or written; a
  hand-edited held document is re-checked on every read by `check_held`
  (`data_sources/held.rs`), so a `default` connection slipped in outside
  this API is refused there too.
- **`DataSourceState`** — the port: `read_data_sources` /
  `write_data_sources`, a whole-document read and a compare-and-swap replace
  (`at: None` meaning "create; refuse if present", per document — the whole
  environment's data sources — never per id), implemented by
  `fabric-platform-git`'s adapter over
  `environments/<environment>/data-sources.yaml`. Not a supertrait of
  `DesiredState`: the two are combined instead by `PlatformRepository`
  (`binding/repository.rs`), `trait PlatformRepository: DesiredState +
  DataSourceState {}` with a blanket impl, which is what
  `PlatformDesiredState::connect` accepts — a caller that already has the
  binding never needs a second one for data sources, and no existing
  `DesiredState` implementor had to grow a `DataSourceState` half it does
  not use.
- **`DataSources`** — the service over that port: `list` reads what is
  declared, unchanged; `declare` computes the revision itself (a caller's
  value is never trusted), plans the complete list the write should produce,
  and returns `Declared::Unchanged` without calling the port at all when
  nothing about the declaration differs from what is held. The planning rule
  itself is a pure function (`data_sources/plan.rs`), tested on its own.

`DataSources` is a second service beside `PlatformManagement`, not a method on
it — see `fabric-control-plane`'s `PlatformBinding`, which holds one of each
over the same late-bound repository. Neither one's tests need the other's
ports.

## Getting started

```rust,ignore
use std::sync::Arc;
use fabric_platform_management::{PlatformManagement, PlatformDesiredState};

let binding = PlatformDesiredState::unconnected(); // nothing connected yet
// an operator connects one later:
binding.connect(Arc::new(the_platform_git_repository)).await;

let service = PlatformManagement::new(registry, charts, binding, clock)
    .with_observer(the_kubernetes_observer); // optional; omit and `running` stays `Unknown`

let statuses = service.statuses("production").await?;
```

## Common tasks

- **Reading a component's situation** — `PlatformManagement::status`/`statuses`.
  Never call `reconcile` from a read path; it writes on `Decision::Advance`.
- **Running a sweep** — call `PlatformManagement::sweep(environment,
  &SweepState)` on whatever cadence the host chooses. Check `SweepResult`:
  `AlreadyRunning` (a previous sweep for this state was still in flight —
  the record is untouched, since a skipped sweep found nothing *because it
  did not look*) and `NotConnected` are both distinct from `Ran(Sweep)`.
- **Pausing/resuming/rolling back from an operator action** — call
  `PlatformManagement::pause`/`resume`/`roll_back` directly; these are
  operator-triggered writes with their own authorization (upstream of this
  crate), never reachable through the selector's `advance` path.
- **Adding a new artifact kind** — extend `ArtifactSource`/`ArtifactKind`
  deliberately (a closed set on purpose — see `artifact.rs`'s module docs)
  and provide both a `Registry`- and `ChartIndex`-shaped discovery path, or
  a third port if genuinely neither fits.
- **Wiring up running-version evidence** — implement `DeploymentObserver`
  (see `fabric-deployment-kubernetes` for the shipped Kubernetes adapter) and
  attach it with `PlatformManagement::with_observer`. The port is optional by
  design: a deployment with no observation binding configured keeps working
  exactly as before, with `Running::Unknown`.
- **Declaring or correcting a data source** — call
  `DataSources::declare(environment, declaration, at)`. `at` is the revision
  the caller read; `None` is valid only when nothing has been declared for
  the *environment* yet — per document, not per id, since one document holds
  every data source an environment declares — and a stale value is a
  `DesiredStateError::Conflict`. The binding this platform runs always tags
  a revision, even for an environment with nothing declared, so in
  production `at` is never actually `None`; see `PlatformDesiredState`.

## Gotchas

- The crate is `fabric-platform-management` (hyphen); the Rust identifier —
  and the `RUST_LOG` filter target — is `fabric_platform_management`
  (underscore): `RUST_LOG=info,fabric_platform_management=debug`.
- `Discovery::newer` is **not** "the latest available version" — it is the
  newest eligible version *newer than desired*. Nothing here observes
  whether the desired version is still published, so reporting it as
  "available" is a claim this type is not entitled to make (the console used
  to render `Available —` about an environment already running the newest
  preview there was).
- `Reason::UndefinedStablePolicy` is a deliberate fail-closed gap, not a bug
  waiting to be fixed casually: nobody has decided what an automatic
  *stable* advance is allowed to do (patch? minor? never major?), so until
  that is decided, a stable `Automatic` component advances nothing.
- `is_series` compares only `major.minor.patch` — build metadata and
  prerelease identifiers are irrelevant to "same line".
- `PlatformDesiredState`'s drain, generation tagging and operation ordering
  are **one process's**. There is no leader election and no shared
  coordination across replicas; a second control-plane instance sweeping
  concurrently is handled by the write's own staleness check, not by this
  crate arbitrating between processes.
- `tokio::task_local!`-style budgeting does **not** live in this crate — it
  lives in the adapter (`fabric-platform-git`). This crate's contract
  (`DesiredState`'s module docs) only *requires* every implementation to
  bound its own operations by refusing to start work rather than abandoning
  work already sent; it does not implement that bound itself.
- `ComponentStatus::assemble` and the pause/resume/rollback service methods
  build their result from what was just written, never a fresh read — a
  second read there would race the write this call just performed. Every one
  of those paths leaves `running: Running::Unknown` and `observation: None`:
  only `status`/`statuses` ever consult the observer.
- `Running::Observed(String)` is filled in from `DeploymentObservation.version`
  and nothing else. A missing or disagreeing observation, or no observer
  attached at all, all collapse to `Running::Unknown` — this crate does not
  distinguish "never asked" from "asked and got nothing coherent" in the
  `running` field itself (the richer detail, when there is any, lives in
  `ComponentStatus::observation`).
- `PlatformError::InvalidDataSource` wraps a `DataSourceRule` transparently
  — it is a validation refusal, not a repository failure, and is kept apart
  from `PlatformError::DesiredState` for the same reason `NotAdvancing` is:
  the request was understood and refused on its own terms before anything
  was read.
