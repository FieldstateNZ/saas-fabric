# fabric-platform-git — LLM context

Atomic multi-file desired-state mutation in the platform repository, over
the Git Data API, and the `fabric_platform_management::DesiredState` adapter
built on it. In neither plane (see `docs/architecture/crate-dependencies.md`).
Depends on `fabric-core`, `fabric-git-host` (shared App-credential exchange),
`fabric-platform-management` (the port implemented; also the source of
`Channel`, `Hold`, `UpdatePolicy`, re-declared in `components.rs` via `pub use
fabric_platform_management::{Channel, Hold, UpdatePolicy}` — only `Hold` and
`UpdatePolicy` are re-exported onward from `lib.rs`; `Channel` is used in
`Component`'s field type but a caller names it via
`fabric_platform_management::Channel` directly), plus `async-trait`, `base64`,
`reqwest`, `serde`, `serde_json`, `serde_norway` (YAML), `thiserror`, `tokio`
(declares `rt` and `time`; links the workspace's additive `macros`,
`rt-multi-thread`, `sync`, `time` set, as its Cargo.toml says), `tracing`.

## Public surface (all re-exported from `lib.rs`)

- `PlatformGitRepository` — `new(config: &PlatformRepositoryConfig,
  credential: GitCredential, clock: Arc<dyn Clock>) -> Result<Self, String>`.
  `describe() -> String` (`"{owner}/{repository} on {branch}"` — no
  credential, no API base URL). Implements `fabric_platform_management::DesiredState`.
- `PlatformRepositoryConfig { api_base_url, owner, repository, branch,
  http_timeout_seconds, operation_timeout_seconds }`. `.validate()`. No
  path-prefix or file-list field — which files a change touches is decided
  per-change by the caller and by `pinnedIn` in the manifest itself.
- `update_files_atomically(base: &CommitRevision, changes: &[FileChange],
  message: &str) -> Result<CommitRevision, PlatformGitError>` (method on
  `PlatformGitRepository`) — the one write primitive. `ATTEMPTS = 4`. Refuses
  an empty `changes` slice as `PlatformGitError::Rejected` before touching
  the network.
- `components_manifest(environment) -> Result<Manifest, PlatformGitError>`,
  `set_component_desired_state(environment, component, wanted:
  &WantedVersion, at: &DesiredRevision, message) -> CommitRevision`,
  `roll_back_component(environment, component, wanted: &WantedVersion, hold:
  &Hold, at: &DesiredRevision, message) -> CommitRevision`,
  `set_component_hold(environment, component, hold: Option<&Hold>, at:
  &DesiredRevision, message) -> CommitRevision` — all methods on
  `PlatformGitRepository`, all delegate to `write_desired`/direct writes.
- `Manifest { schema_version: u32, environment: String, managed_roots:
  Vec<String>, components: BTreeMap<String, Component> }`.
  `SCHEMA_VERSION: u32 = 2`. `#[serde(rename_all = "camelCase")]` throughout
  this module.
- `Component { artifact: Artifact, channel: Channel, update: UpdatePolicy,
  desired: Desired, pinned_in: Vec<Pin>, hold: Option<Hold> }`.
- `Artifact` — `Oci { source_revision: String, images: BTreeMap<String,
  ImagePin> }` | `Helm { repository: String, chart: String }` (`tag =
  "type"`, `deny_unknown_fields`, closed set). `.describe() -> &'static
  str`. `.parse_version(text) -> Option<Version>` — dispatches to
  `Version::parse` (OCI, refuses build metadata) or `Version::parse_chart`
  (Helm, keeps it).
- `ImagePin { repository: String, digest: String }`.
- `Desired { version: String }` — once per component, not per image.
- `Pin` — `KustomizeImage { path, image }` |
  `ArgoTargetRevision { path, repository, chart }` (`tag = "renderer"`,
  `rename_all = "kebab-case"`, `deny_unknown_fields`). `.path() -> &str`,
  `.describe() -> &'static str`.
- `ComponentVersion`, `ImageDigest`, `WantedVersion` (from `desired/inputs.rs`)
  — the caller-facing "what to write" shapes; `WantedVersion` wraps either an
  OCI `ComponentVersion` (with images by role) or a `Chart { repository,
  chart, version }`, and is what `port/wanted.rs::wanted_from(&Release)`
  produces from the domain's `Release`.
- `PlatformGitError` — `Conflict { path }`, `Contended`, `NotFound { what }`,
  `NotPermitted`, `Unavailable { detail }`, `Rejected { detail }`. `From<TokenError>`
  maps `fabric_git_host::TokenError` 1:1 (`NotPermitted`→`NotPermitted`,
  `Unavailable`→`Unavailable`, `Rejected`→`Rejected`).
- `FileChange { path: String, text: String, expected: Option<FileRevision> }`.
- `FileRevision(String)` — a content hash (the file's blob `sha`); moves only
  when *that file* changes.
- `CommitRevision(String)` — a whole-branch revision, returned from a
  successful write.
- `StoredFile { path, text, revision: FileRevision }`.

## Internal modules

- `atomic.rs` — `update_files_atomically`, `refuse_if_a_written_path_moved`,
  `ATTEMPTS = 4`.
- `components.rs` + `components/{artifact,document,model,overlay,pin,pinning}.rs`
  + `components/argo/` — the manifest schema and its YAML document handling.
  `document.rs`: `Document::{parse,render}` (serde_norway round-trip;
  `parse` first deserialises a permissive `Versioned { schema_version }`
  shape to check the version *before* the full `Manifest` parse, so a
  version mismatch is reported as that, not as an unrelated missing-field
  error). `overlay.rs`: `repin` — rewrites a Kustomize `images:` entry by
  locating its `- name: {repository}` line and replacing its indented
  `newTag`/`digest` keys in place; refuses unless exactly one entry matches.
  `pinning.rs`: `check_writable` (four rules: repository-relative, no `..`
  traversal, under a declared `managedRoot`, `.yaml`/`.yml` suffix — two
  more, existence and actually-pins-the-image, are checked by the caller
  that reads/renders the file). `argo/`: `retarget` and its
  line/position-tracking scalar rewrite (`entry.rs`, `lines.rs`,
  `position.rs`, `scalar.rs`, `seen.rs`, `value.rs`, `walk.rs`) — matches on
  **both** `repository` and `chart` (never chart name alone), edits only the
  matching source's `targetRevision` scalar, and refuses (rather than
  guesses) on zero or multiple matching sources, a source with no
  `targetRevision`, or a shape it cannot structurally parse.
- `config.rs` — `PlatformRepositoryConfig` and its validation.
- `desired.rs` + `desired/{identity,inputs,plan,render,write}.rs` — the
  higher-level write operations. `identity.rs::check_release` refuses a
  `WantedVersion` that does not match what the component's `Artifact`
  publishes, via an exhaustive match over all four `(Artifact, WantedVersion)`
  combinations (no wildcard arm, so a third `Artifact` variant fails to
  compile rather than silently refusing every release). `plan.rs::rewrite_pins`
  computes the `FileChange`s for every declared `Pin` (grouping by path, since
  two roles can share one overlay file); `plan::apply` updates the in-memory
  `Component`'s `desired.version` and, for OCI, `source_revision` and each
  image's `digest` — a chart has nothing else to update. `render.rs::render`
  matches `(Pin, Artifact, WantedVersion)` and dispatches to `repin`/`retarget`;
  returns `Ok(None)` when a pin has nothing to write for this release (an
  image the release does not carry). `write.rs::write_desired` is the shared
  body both `advance`-shaped and `roll_back`-shaped writes call, via
  `HoldChange::{Keep, Set}`; it re-checks `read.stored.revision.as_str() ==
  at.as_str()` against the manifest file specifically (the precondition the
  decision was actually taken against, not a fresh read of the adapter's own).
- `errors.rs` — `PlatformGitError`, `From<TokenError>`.
- `hold.rs` — `set_component_hold` (pause/resume — touches only the hold
  field on one component entry, re-renders the whole manifest, never reads
  or writes a `pinnedIn` file).
- `host.rs` + `host/{failures,objects,reads,refs,sending,wire}.rs` — the Git
  Data API client. `API_VERSION = "2022-11-28"` (sent as
  `X-GitHub-Api-Version`). `sending.rs`: `send` retries once on `401` by
  calling `self.bearers.invalidate()` under a `tokio::time::timeout(self.bearer_allowance(),
  ...)`, then re-attempts; `attempt` calls `refuse_if_the_budget_is_spent()`
  both before and after acquiring the bearer (the bearer acquisition itself
  is wrapped in `tokio::time::timeout(self.bearer_allowance(), ...)`), then
  sends once. `refs.rs`: `RefUpdate::{Applied, NotFastForward}` — a `409`
  from `update_ref` becomes `NotFastForward`, an ordinary value, never a
  `PlatformGitError`; every other status (including `422`) is a genuine
  `PlatformGitError` via `status_failure`, so a misconfiguration can never be
  reinterpreted as contention. `reads.rs`: `head()`, `read(path, at)`,
  `revision_at(path, at)` (absence → `Ok(None)`, not an error — the retry
  path treats a deleted file as "moved").
- `manifest.rs` — `read_manifest` (reads + parses + checks the manifest
  actually describes the requested environment), `manifest_path(environment)
  -> String` (`environments/{environment}/components.yaml`, the one place
  this path is spelled out).
- `model.rs` — `FileRevision`, `CommitRevision`, `StoredFile`, `FileChange`.
- `port.rs` + `port/{budget,budget/bearer,errors,reading,wanted}.rs` — the
  `impl DesiredState for PlatformGitRepository` (all six methods —
  `components`, `component`, `advance`, `roll_back`, `pause`, `resume` —
  wrapped in `within_budget`). `budget.rs`: `within_budget`
  (`tokio::task_local! STARTED: Instant`, reuses an already-scoped start
  rather than nesting a second budget), `refuse_if_the_budget_is_spent`,
  `out_of_budget`. `budget/bearer.rs`: `bearer_allowance` — what remains
  until `operation_timeout_seconds + http_timeout_seconds` has elapsed since
  the operation began (one `http_timeout_seconds` when unbudgeted), anchored
  to the operation's absolute start rather than a fresh window each call.
  `wanted.rs`: `wanted_from(&Release) -> WantedVersion`.

## Hard invariants — do not break

1. **No path in this crate ever sends `force: true`** on a ref update, and
   no public API can request one.
2. **A write is one commit or none.** `update_files_atomically` either lands
   every file in `changes` in a single tree/commit, or nothing is written.
3. **A non-fast-forward triggers re-checking only the paths *this write* is
   editing**, never a blanket refusal or a blind retry. Unrelated concurrent
   commits cost a retry (bounded at `ATTEMPTS = 4`, then `Contended`); a
   genuine collision on an edited path is `Conflict`, immediately.
4. **The operation budget gates starting a request; it never cancels one
   already sent.** `within_budget`/`refuse_if_the_budget_is_spent` must
   never be replaced with a `tokio::time::timeout` wrapping a whole
   operation — that reintroduces the exact bug (a dropped future releasing
   the platform binding's lock with a write possibly in flight) this design
   fixes.
5. **`check_writable`'s rules apply to every `pinnedIn` entry**, even
   though the manifest is trusted desired state — this is a confused-deputy
   defence, not an input-validation one.
6. **`Pin`'s renderer is the enum tag**, never a generic path/pointer field
   beside a renderer name. A new renderer is a new variant with exactly the
   fields it needs.
7. **`Artifact::parse_version` dispatch is per-artifact-kind, never global.**
   An OCI version must not carry build metadata; a chart version may.
8. **`write_desired`'s conflict check compares the manifest file's own
   revision against the caller's `at`**, not a fresh read of the adapter's
   own state against itself — the latter only proves nothing changed during
   the write, not that the write is still being applied to the state it was
   decided against.

## Notes

- `tests/atomic_update.rs`, `tests/component_desired_state.rs` and
  `tests/operation_budget.rs` drive a fake HTTP server
  (`tests/support/fake_platform_host.rs`, `tests/support/http_server.rs`)
  rather than a real GitHub — the budget tests in particular measure and
  delay against the clock to prove the timing guarantees.
- `fabric-platform-git` and `fabric-client-git` share `fabric-git-host` for
  the credential exchange only. There is no dependency edge between the two
  adapters, and none is permitted — see
  `docs/architecture/crate-dependencies.md`'s "`fabric-git-host` is shared
  because the credential is, and the integration is not".
- The manifest's `pinnedIn` list is itself validated by the platform
  repository's own CI, redundantly with `check_writable` here — CI proves
  coherence at the commit it ran on; this crate re-checks the state it
  actually reads, which may not be a state CI has seen yet.
