# fabric-platform-git

Atomic, multi-file desired-state mutation in the platform repository, over
the Git Data API — and the `DesiredState` adapter that lets
`fabric-platform-management` read and move it. This is the platform's own
equivalent of `fabric-client-git`, using the same credential mechanism
(`fabric-git-host`) against a completely separate GitHub App and repository.

Sits in neither plane (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
Depends on `fabric-core`, `fabric-git-host` (the App credential) and
`fabric-platform-management` (the update-policy rules this crate serialises,
and the `DesiredState` port it implements).

## Why this crate exists

Moving a platform component's version touches more than one file: the
version record in `environments/<env>/components.yaml`, and every overlay
that pins one of its images (a Kustomize `images:` entry, an Argo
`targetRevision`). Written as separate commits, the branch would pass
through states nobody chose — a record naming a version the overlays do not
yet deploy, or two of three images moved — and Argo CD is entitled to
reconcile *any* commit on the branch an environment follows. So every change
this crate makes is **one tree, one commit, one ref update**, or none of
them.

GitHub's ref-update endpoint has no expected-old-SHA parameter; it takes only
the new SHA and `force`. With `force=false` it requires a fast-forward, so a
commit whose parent is a head that has since moved cannot land, and the host
answers `409`. That `409` is the *only* concurrency signal this crate gets,
and it deliberately does not stop there — see "Concurrency" below.

## Key concepts

- **`update_files_atomically`** (`atomic.rs`) — the one write primitive
  everything else is built from. Creates every blob once (blobs are
  content-addressed, so this happens outside the retry loop), then loops up
  to 4 times (`ATTEMPTS`): build a tree and commit on the current head, try
  the ref update, and on a non-fast-forward re-read the head and check
  whether any of *this write's own paths* moved. If none did, rebuild on the
  new head and retry. If one did, refuse immediately as `Conflict` — never
  overwrite a change to a file this write is editing.
- **Conflict vs. Contended, and they are not the same answer**
  (`errors.rs`) — `Conflict { path }` means a file this write was editing
  changed underneath it: a person has to decide what to do about the other
  change. `Contended` means the branch kept moving for *unrelated* reasons
  and the write ran out of retry attempts: nothing needs deciding, trying
  again later is the whole remedy. Collapsing the two would either ask an
  operator to resolve a busy branch, or silently retry past a real
  disagreement.
- **There is no `force`.** No path in this crate sends `force: true`, and no
  caller can ask for one — forcing is how a platform repository loses a
  commit nobody knew about. A losing attempt's unreachable blobs/tree/commit
  objects are left for the Git host's own garbage collection rather than
  cleaned up — deleting them would add failure modes to the recovery path of
  a failure, to tidy something nobody can see.
- **The manifest schema (`components.rs`)** — `Manifest { schema_version,
  environment, managed_roots, components: BTreeMap<String, Component> }`.
  `SCHEMA_VERSION = 2`; a manifest declaring a different version is refused
  rather than half-understood. `Component.pinned_in: Vec<Pin>` is the
  component's own statement of every place its version is written — this
  crate never guesses a file layout. `Pin`'s *renderer* is the enum tag
  itself (`KustomizeImage { path, image }` |
  `ArgoTargetRevision { path, repository, chart }`), not a field beside a
  generic path — a `jsonPath`/regex/YAML-pointer escape hatch here would turn
  a trusted platform document into an arbitrary repository-edit engine.
- **`check_writable` (`components/pinning.rs`)** — four rules checked here,
  plus two more checked where a pin's file is actually read and rendered
  (exists; actually pins the image) — six in total bounding which files this
  crate will write, even though `pinnedIn` is *trusted* desired state:
  repository-relative, no `..` traversal, under a declared `managedRoot`, a
  `.yaml`/`.yml` file. This is defence against a mistake in a trusted
  document, not against an untrusted caller: a wrong `pinnedIn` entry must
  not make this crate a confused deputy editing a CI workflow or a README.
- **The operation budget (`port/budget.rs`)** — every `DesiredState` method
  is wrapped in `within_budget`, which uses a `tokio::task_local!` to record
  when the *operation* (not any one request) began, and
  `refuse_if_the_budget_is_spent` is checked before every request the
  operation is about to send. This deliberately is **not**
  `tokio::time::timeout`: a timeout drops the future, and a future dropped
  mid-write releases `fabric-platform-management`'s binding lock with a
  write possibly already on the wire — so a disconnect could return while an
  abandoned commit still lands. The budget instead refuses to *start* the
  next request once spent, so an operation always ends within
  `operation_timeout_seconds` plus one `http_timeout_seconds`, and a write
  already sent always runs to its own outcome.
- **The Argo YAML rewrite (`components/argo/`)** — `retarget` rewrites only
  the `targetRevision` scalar of the source that names a given chart
  repository + chart, via a hand-written line/position-tracking walk rather
  than a full parse-and-reserialise. An Argo `Application` typically has
  *two* sources (the chart, and a values repository with its own unrelated
  `targetRevision`), and matching on chart name alone would not be enough in
  general — both `repository` and `chart` must agree.

## How the pieces fit

```text
fabric-platform-management::DesiredState   the port this crate implements
        |
   port.rs                                  within_budget() wraps every method
        |
   desired.rs / hold.rs                     read-manifest, check identity, plan
        |                                   pin rewrites, render, write-atomically
   atomic.rs                                update_files_atomically: blobs -> tree
        |                                   -> commit -> ref, with the 409 retry
   host.rs (+ host/*)                       the Git Data API client
        |
   fabric-git-host::BearerSource            mints the App's installation token
```

## Getting started

```rust,ignore
use std::sync::Arc;
use fabric_core::SystemClock;
use fabric_git_host::GitCredential;
use fabric_platform_git::{PlatformGitRepository, PlatformRepositoryConfig};

let config = PlatformRepositoryConfig {
    api_base_url: "https://api.github.com".to_owned(),
    owner: "FieldstateNZ".to_owned(),
    repository: "saas-fabric-platform".to_owned(),
    branch: "main".to_owned(),
    http_timeout_seconds: 10,
    operation_timeout_seconds: 20,
};

let repository = PlatformGitRepository::new(
    &config,
    GitCredential::app(app_id, installation_id, private_key_pem),
    SystemClock::shared(),
)?;

// implements fabric_platform_management::DesiredState directly:
let binding = fabric_platform_management::PlatformDesiredState::unconnected();
binding.connect(Arc::new(repository)).await;
```

`config.operation_timeout_seconds + config.http_timeout_seconds` must be
strictly less than whatever request timeout sits in front of an operator's
disconnect/rebind call — this is the sum the host is expected to check at
startup (see `docs/architecture/control-plane.md`'s "A decision is applied
to the state it was taken against").

## Common tasks

- **Reading an environment's manifest directly** (outside the `DesiredState`
  port) — `PlatformGitRepository::components_manifest(environment)`.
- **Moving a component's version** —
  `set_component_desired_state`/`roll_back_component` on
  `PlatformGitRepository` directly, or (preferred) through
  `fabric_platform_management::DesiredState::advance`/`roll_back`, which adds
  the operation budget and the platform-binding's generation tagging.
- **Adding a new pin renderer** — add a variant to `Pin` in
  `components/pin.rs` with exactly the fields that renderer needs (no
  optional/shared fields), and a matching branch in `desired/render.rs` (and
  a new module beside `components/argo/` if the renderer needs one).
  `Pin::path()` and `Pin::describe()` must stay total over every variant.
- **Debugging a `Contended` error** — it means the platform repository's
  branch is being written to faster than 4 retries can keep up with. Check
  what else is committing to it; this is not a bug in this crate to fix by
  raising `ATTEMPTS` blindly.

## Gotchas

- The crate is `fabric-platform-git` (hyphen); the Rust identifier — and the
  `RUST_LOG` filter target — is `fabric_platform_git` (underscore):
  `RUST_LOG=info,fabric_platform_git=debug`.
- A transport failure from `update_ref` is **never retried inside this
  crate**. If the ref update actually landed on the host but the response
  timed out on the way back, retrying here would find the just-written paths
  carrying their new revisions and report a `Conflict` against the caller's
  *own* change. `Unavailable` is returned instead, leaving the caller to
  re-read — at which point it finds its change already applied and has
  nothing left to do.
- `operation_timeout_seconds` bounds when a request may *start*, never a
  request already in flight — only acquiring a bearer token is ever
  genuinely cut short (via a `tokio::time::timeout` in `host/sending.rs`,
  bounded by `port/budget/bearer.rs::bearer_allowance`), because a token
  exchange writes nothing to the repository if abandoned.
- `Artifact::parse_version` dispatches on the artifact kind, not on the text
  being parsed: an OCI component's version is refused if it carries build
  metadata (illegal in an OCI tag); a Helm component's is not. Fixing this to
  "one global parser" would make a chart version written *with* build
  metadata (which chart repositories publish routinely) unreadable on its
  next read.
- `serde_norway` (not `serde_yaml`) is the YAML library. `serde_yaml`'s
  author has archived it; `serde_norway` is the maintained fork
  (`docs/architecture/dependency-policy.md`), also used by
  `fabric-client-model` and `fabric-registry`.
- `check_writable`'s rules are duplicated in the platform repository's own
  CI, deliberately: CI proves the manifest is coherent *at the commit it ran
  on*; this crate re-applies the same rules to whatever was actually read at
  request time, which may be a state CI never saw.
- The ref update itself never returns a Git-level "conflict" error type — a
  non-fast-forward `409` comes back from `update_ref` as
  `RefUpdate::NotFastForward`, an ordinary enum value, not a
  `PlatformGitError`. Every *other* status from that call (including a
  `422`) is a genuine `PlatformGitError`, precisely so a misconfiguration
  can never be reinterpreted as contention and retried forever.
