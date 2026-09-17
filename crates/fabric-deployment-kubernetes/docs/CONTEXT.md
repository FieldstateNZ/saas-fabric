# fabric-deployment-kubernetes — LLM context

The shipped `fabric_platform_management::DeploymentObserver` implementation
(ADR 0022): read-only Kubernetes deployment evidence, over plain HTTPS with
`reqwest` and the pod's own projected service-account token — never a `kube`
crate (`scripts/check_architecture.py`'s `CONTROL_PLANE_CLIENTS` bans
`kube`/`k8s-openapi`/`kube-client`/`kube-runtime` workspace-wide). In the
control plane (`CONTROL_PLANE` set in `scripts/check_architecture.py`); only
the control-plane composition root binds it. Depends on `fabric-core`,
`fabric-platform-management` (the port it implements and the
`DeploymentHealth`/`DeploymentObservation`/`WorkloadObservation`/`Version`
types it returns), `async-trait`, `reqwest`, `serde`, `serde_json`, `tokio`
(`fs` feature — for reading the token file async).

## Public surface (re-exported from `lib.rs`)

- `WorkloadTarget { namespace: String, deployment: String, container:
  String, repository: String }` (`Clone`, `Debug`, `Deserialize`,
  `deny_unknown_fields`). Deployment-owned configuration, never constructed
  from an API request. `pub(crate) fn validate(&self) -> Result<(), String>`
  — `namespace`/`deployment`/`container` must be non-empty, ≤253 bytes, and
  only `[a-z0-9-.]`; `repository` must be non-empty and free of `@`, `?`,
  `#`, or whitespace.
- `KubernetesObserver` — implements
  `fabric_platform_management::DeploymentObserver`. Only constructor:
  `in_cluster(environment: &str, targets: BTreeMap<String,
  Vec<WorkloadTarget>>, clock: Arc<dyn Clock>) -> Result<Self, String>`.
  Refuses a component with 0 or >16 targets, refuses any target that fails
  `validate()`, and refuses if the cluster CA
  (`/var/run/secrets/kubernetes.io/serviceaccount/ca.crt`) cannot be read or
  parsed. `async fn observe(&self, environment: &str, component: &str) ->
  Option<DeploymentObservation>` — `None` if `environment` does not match
  what this observer was built with, or if `component` names no configured
  targets; otherwise runs every target's read under one 5-second
  `tokio::time::timeout` and always returns `Some`.

## Internal modules

- `config.rs` — `WorkloadTarget` and `validate()` alone.
- `observer.rs` — `KubernetesObserver { client: Client, environment: String,
  targets: BTreeMap<String, Vec<WorkloadTarget>>, clock: Arc<dyn Clock> }`.
  - `in_cluster`: builds the `reqwest::Client` with the CA pinned via
    `add_root_certificate`, `redirect(Policy::none())` (no redirects ever
    followed), and a 4-second per-request timeout;
    `base = "https://kubernetes.default.svc"`,
    `token_file = "/var/run/secrets/kubernetes.io/serviceaccount/token"`.
  - `workload(&self, target) -> Result<WorkloadObservation, String>` — `GET
    /apis/apps/v1/namespaces/{ns}/deployments/{name}` (`before`); refuses
    (`Err`) if `spec.selector.matchLabels` is empty or
    `matchExpressions` is non-empty (only a plain equality selector is
    supported); builds `k=v,k=v` from `matchLabels`; `list`s
    `/apis/apps/v1/namespaces/{ns}/replicasets` and
    `/api/v1/namespaces/{ns}/pods` with that selector; re-`GET`s the
    Deployment (`after`); refuses if `metadata.resourceVersion` or
    `metadata.uid` differ between `before` and `after` (a concurrent
    change); otherwise delegates to `evaluate::evaluate`.
  - `workloads(&self, targets: &[WorkloadTarget]) -> Vec<WorkloadObservation>`
    — calls `workload` for each target; a failure becomes
    `WorkloadObservation { health: Unavailable, versions: vec![],
    desired_replicas: None, ready_replicas: 0, detail: Some(message) }`
    rather than aborting the rest.
  - `observe`: wraps `workloads(targets)` in
    `tokio::time::timeout(Duration::from_secs(5), ...)`; on timeout, treats
    it as zero workloads with `detail: Some("Deployment observation timed
    out; no current evidence is available.")`; calls
    `summary::summarize(workloads, clock.now_unix_seconds(), detail)`.
- `client.rs` — `pub(crate) struct Client { http: reqwest::Client, base:
  String, token_file: PathBuf }`. `async fn get<T: DeserializeOwned>(&self,
  path: &str, selector: Option<&str>) -> Result<T, String>` — reads
  `token_file` fresh (`tokio::fs::read_to_string`, trimmed) on **every**
  call, sends one `GET` with `bearer_auth`, and (when `selector` is given)
  query params `labelSelector={selector}&limit=500`. Status mapping: `401`
  or `403` → `"Deployment observation is not permitted."`; `404` →
  `"The configured deployment was not found."`; anything else unsuccessful →
  `"The cluster could not supply deployment evidence."` — never the
  response body. Body streamed in chunks with a 2 MiB cap
  (`2 * 1024 * 1024` bytes); exceeding it is
  `"Deployment evidence exceeded its size limit."`. `async fn list<T>(&self,
  path, selector: &str) -> Result<Vec<T>, String>` — calls `get::<List<T>>`
  and refuses (`"Deployment evidence requires more than one page."`) if
  `metadata.continue` (`ListMetadata::continuation`) is non-empty; never
  follows a second page.
- `wire.rs` — the Kubernetes API JSON shapes this crate reads, all
  `pub(crate)`, all `Deserialize`-only: `Metadata { uid, generation,
  resource_version, deletion_timestamp, owner_references: Vec<Owner> }`,
  `Owner { uid, kind, controller }`, `Deployment { metadata, spec, status }`,
  `DeploymentSpec { replicas, selector: Selector, template: Template }`,
  `Selector { match_labels: BTreeMap<String, String>, match_expressions:
  Vec<serde_json::Value> }` (expressions are read only to check the list is
  empty — never interpreted), `Template { spec: PodSpec }`, `PodSpec {
  containers: Vec<Container> }`, `Container { name, image }`,
  `DeploymentStatus { observed_generation, replicas, updated_replicas,
  available_replicas, ready_replicas, conditions: Vec<Condition> }`,
  `Condition { r#type, status }`, `ReplicaSet { metadata }`, `Pod {
  metadata, spec, status: PodStatus }`, `PodStatus { container_statuses:
  Vec<ContainerStatus>, conditions }`, `ContainerStatus { name, ready,
  image_id, state: BTreeMap<String, serde_json::Value> }` (`state`'s keys —
  `running`/`waiting`/`terminated` — are checked only for the presence of
  `"running"`), `List<T> { metadata: ListMetadata, items: Vec<T> }`,
  `ListMetadata { continuation: String }` (`#[serde(rename = "continue")]`).
- `evaluate.rs` — `pub(crate) fn evaluate(target: &WorkloadTarget,
  deployment: &Deployment, sets: &[ReplicaSet], pods: &[Pod]) ->
  WorkloadObservation`. Computes `owned` ReplicaSet UIDs (an
  `ownerReferences` entry with `controller: true`, `kind: "Deployment"`,
  `uid == deployment.metadata.uid`), then `pods` owned by one of those
  ReplicaSets the same way. `expected` is `image::pinned` applied to the
  target container's **spec** image. Per owned pod: only a container status
  whose `state` map contains the key `"running"` **and** whose `image_id`
  matches the pinned digest (`image::running_digest`) contributes a version;
  it counts toward `ready` only if the container status is `ready`, a pod
  condition `Ready: True` is present, and `deletionTimestamp` is absent; it
  counts toward `matching` only if its `(version, digest)` equals
  `expected`. Health, in order: `Degraded` if a `Progressing: False` or
  `ReplicaFailure: True` condition is present; `Stopped` if `desired == 0`
  and the controller has observed the current generation (`observed_generation
  >= metadata.generation`, no `deletionTimestamp`) and there are zero owned
  pods and `status.replicas == 0`; `Unavailable` if `expected` is `None`
  (the spec container's image is not `repository:version@sha256:<64 hex>`
  for the target's own `repository`); `Healthy` if observed, `desired > 0`,
  and `matching`/`pods.len()`/`replicas`/`updated_replicas`/`ready_replicas`/
  `available_replicas` all equal `desired`; otherwise `Progressing`. `detail`
  is `Some(...)` only when `health == Unavailable`.
- `image.rs` — `pub(crate) fn pinned(image: &str, repository: &str) ->
  Option<(&str, &str)>`: splits on `@` (tag half, digest half); the tag half
  must start with `{repository}:`; the digest half must be `sha256:` plus
  exactly 64 hex characters; the version (with an optional leading `v`
  stripped) must parse via `fabric_platform_management::Version::parse` —
  the OCI grammar, which refuses build metadata. Returns `(version_as_written,
  full_digest_string)`. `pub(crate) fn running_digest(image_id: &str,
  digest: &str) -> bool` — `true` if `image_id` equals `digest` exactly, or
  if splitting `image_id` on the last `@` yields a suffix equal to `digest`;
  a running `imageID` naming a different digest (e.g. an unresolved
  multi-architecture index digest) is `false`, never treated as a match.
- `summary.rs` — `pub(crate) fn summarize(workloads: Vec<WorkloadObservation>,
  at: u64, detail: Option<String>) -> DeploymentObservation`. Health
  precedence: `Unavailable` if `workloads` is empty or any workload is
  `Unavailable`; else `Degraded` if any workload is `Degraded`; else
  `Progressing` if any workload is `Progressing`; else `Stopped` if *every*
  workload is `Stopped`; else (every workload `Healthy`) a `Healthy`
  candidate. `version`: the single distinct version across workloads whose
  own health is **not** `Stopped`, but only when the candidate health is
  `Healthy` and there is exactly one such distinct version — any other
  count (zero or more than one) yields `None`. If health was `Healthy` but
  `version` ended up `None`, health is downgraded to `Progressing` (a
  "healthy" state that cannot name a single agreed version is not reported
  as healthy). `observed_at_unix_seconds: at`, `workloads`, `detail` are
  carried through unchanged.

## Hard invariants — do not break

1. **No `kube`-family crate may enter this crate's dependency graph.**
   `scripts/check_architecture.py`'s `CONTROL_PLANE_CLIENTS` enforces this
   workspace-wide; this crate is the reason the ban was extended to
   Kubernetes clients (it was originally about Git).
2. **This crate never writes.** `DeploymentObserver::observe` is the only
   thing it implements; there is no path from anything here to a Kubernetes
   write verb, and its evidence is never fed into `fabric-platform-git`'s
   desired-state writes.
3. **The bearer token is re-read from disk on every request, never cached**
   — a projected service-account token's scheduled rotation must take
   effect without a restart.
4. **A concurrent change to the observed Deployment is rejected, not
   blended.** `workload`'s before/after `resourceVersion`+`uid` comparison
   is what makes one `WorkloadObservation` describe one consistent read
   rather than a mix of two.
5. **RBAC narrowing is done twice, and both matter.** The label selector
   narrows what is *requested*; `evaluate::evaluate`'s owner-reference
   filtering narrows what is *trusted*, because native Kubernetes RBAC
   cannot itself restrict a `list` of Pods/ReplicaSets by label.
6. **A version counts as running only with its digest**, never from a tag
   alone (`image::pinned` requires `repository:tag@sha256:<64 hex>`) and
   only when the **running** container's `imageID` matches that digest
   (`image::running_digest`) — the spec's requested image is not evidence of
   what is actually running.
7. **No error message or log line from this crate carries the cluster's
   response body or a credential.** Every failure path in `client.rs` maps
   to one of four fixed strings; the raw status/body is never propagated.
8. **A single component's observation is `Healthy` with a version only when
   every non-stopped workload agrees on exactly one version.** Disagreement,
   missing evidence, or any non-`Stopped`, non-`Healthy` workload collapses
   the whole component's `version` to `None` — see `summary.rs`'s
   precedence order.

## Notes

- Tests are `#[path = "..."] mod tests;` beside the module under test
  (`client_tests.rs`, `evaluate_tests.rs`, `summary_tests.rs`), not inline
  `#[cfg(test)] mod tests { ... }` blocks — the one place this crate departs
  from the inline-test convention the other four crates in this slice
  follow.
- `client_tests.rs` drives a real loopback TCP listener (not a mocked
  `reqwest` transport) to prove token rotation, error-message redaction, and
  pagination refusal against actual HTTP request/response bytes.
- Every request the client sends and the token-read path are documented as
  what ADR 0022 calls "bounded GETs using the pod identity" — there is no
  `POST`/`PUT`/`PATCH`/`DELETE` anywhere in this crate.
- This crate's binding — which components map to which `WorkloadTarget`s —
  is what ADR 0022 calls "an explicit component-to-workload binding" the
  host supplies; nothing here infers a mapping from labels, names, or
  Kubernetes conventions.
