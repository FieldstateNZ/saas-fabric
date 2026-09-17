# fabric-deployment-kubernetes

The shipped implementation of `fabric-platform-management`'s optional,
read-only `DeploymentObserver` port
([ADR 0022](../../../docs/decisions/0022-running-versions-come-from-deployment-evidence.md)):
it turns a small, explicit, deployment-owned list of Kubernetes Deployments
into the running-version evidence the platform panel's `Running` row shows.

It sits in the control plane (`scripts/check_architecture.py`'s
`CONTROL_PLANE` set), and only the control-plane composition root may bind
it. It has no edge to `fabric-client-git`, `fabric-platform-git`, or
anything else that writes desired state — it cannot write, and nothing it
observes is ever used to author a Git commit.

## Why this crate exists

Before this crate, the platform panel's `Running` row was permanently
`Unknown`: a desired release recorded in Git says what an environment is
*asked* to run, never what is actually serving. This crate is the first
integration that closes that gap, by reading live evidence from the cluster
Fabric itself runs on — narrowly, and only for the workloads a deployment
explicitly names.

## Why no `kube` crate

`scripts/check_architecture.py`'s `CONTROL_PLANE_CLIENTS` set bans
`kube`, `k8s-openapi`, `kube-client` and `kube-runtime` from the workspace
**everywhere**, the same way it bans `git2`/`gix`/`gitoxide` — see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md).
A generated Kubernetes client pulls in a large, versioned API surface for a
job this crate needs almost none of: three resource kinds (`Deployment`,
`ReplicaSet`, `Pod`), read-only, over a handful of fields. So this crate
reads the Kubernetes API the same way `fabric-client-git` and
`fabric-platform-git` read Git hosts — plain HTTPS with `reqwest`, and its
own narrow `wire.rs` shapes deserialising only what `evaluate.rs` needs — and
authenticates with the pod's own projected service-account token rather than
a client library's credential-loading machinery.

## Key concepts

- **`WorkloadTarget`** (`config.rs`) — one named container a deployment has
  chosen to expose evidence for: `namespace`, `deployment`, `container`,
  `repository`. `#[serde(deny_unknown_fields)]`, and **never accepted from an
  API caller** — per ADR 0022, "the browser cannot supply cluster addresses,
  namespaces, selectors, credentials or deployment names." It is host
  configuration, read at startup like any other binding.
  `WorkloadTarget::validate` requires DNS-style `namespace`/`deployment`/
  `container` (lowercase alphanumeric, `-`, `.`, non-empty, ≤253 bytes) and a
  `repository` with no `@`, `?`, `#` or whitespace.
- **`KubernetesObserver::in_cluster`** — the only constructor. Takes the
  environment name, a `BTreeMap<String, Vec<WorkloadTarget>>` keyed by
  component (1 to 16 workloads per component; more or fewer is refused),
  and a `Clock`. Reads the projected service-account CA
  (`/var/run/secrets/kubernetes.io/serviceaccount/ca.crt`) once at
  construction and builds an HTTP client that trusts only it, follows **no**
  redirects, and times each request out at 4 seconds. The bearer token, by
  contrast, is read fresh from
  `/var/run/secrets/kubernetes.io/serviceaccount/token` on **every request**
  (`client.rs::Client::get`) — never cached — so a projected token's
  scheduled rotation is picked up without restarting the process.
- **Evidence, not a snapshot.** `KubernetesObserver::workload` reads the
  named `Deployment` once, lists the `ReplicaSet`s and `Pod`s matching its
  label selector, then re-reads the `Deployment` and refuses (as a failed
  observation for that workload) if its `resourceVersion` or `uid` changed
  in between — a definition change mid-read must not be blended with
  evidence read against the version before it, per ADR 0022's "second
  Deployment read rejects a concurrent change."
- **Ownership, not label trust.** Kubernetes' native RBAC cannot restrict a
  `list` of Pods or ReplicaSets by label — a role that can list Pods in a
  namespace can list every Pod there. So `evaluate.rs` re-derives which
  ReplicaSets are actually owned by *this* Deployment (`ownerReferences`
  naming it as `controller`), and which Pods are actually owned by one of
  *those* ReplicaSets, before any of them count as evidence. The label
  selector narrows the request; the owner chain narrows what the response is
  trusted to mean.
- **A version counts only with its digest.** `image.rs::pinned` accepts an
  image reference only in `repository:tag@sha256:<64 hex>` form — a bare tag
  is not evidence, because a tag is mutable and a digest is not. A running
  container only contributes that version if its **running** `imageID`
  matches the same digest (`image::running_digest`) — the version label on
  the spec is what the Deployment *asks* for, and this crate never trusts
  that alone. A running `imageID` that names a different digest, most often
  because it is a multi-architecture index digest whose specific platform
  manifest is not yet resolved by this crate, is treated as unconfirmed, not
  as a match.
- **What makes one workload `Healthy`** (`evaluate.rs`) — every one of:
  the Deployment's `status.observedGeneration` has caught up with its
  `metadata.generation` (and it is not being deleted); the desired replica
  count is greater than zero; every counted, ready, non-terminating,
  owned pod is running the exact expected `(version, digest)`; and
  `replicas`/`updatedReplicas`/`readyReplicas`/`availableReplicas` all equal
  the desired count. Anything short of that — some ready, some not; the
  wrong digest; the controller still converging — is `Progressing`, not a
  partial success. A `Progressing: False` or `ReplicaFailure: True`
  condition makes it `Degraded` outright, ahead of every other check. A
  Deployment legitimately scaled to zero, fully converged and empty of pods,
  is `Stopped`, not `Unavailable` or `Healthy`.
- **What one component's observation means** (`summary.rs::summarize`) —
  every configured workload is evidence towards one answer, in strict
  precedence: any workload `Unavailable` (or the workload list itself empty)
  makes the whole observation `Unavailable`; failing that, any `Degraded`
  wins; failing that, any `Progressing`; only if every workload agrees
  `Stopped` is the component `Stopped`; otherwise it is a candidate for
  `Healthy`. A **running version** is reported only when the result is
  `Healthy` *and* every workload that is not `Stopped` agrees on exactly one
  version — a scaled-to-zero runtime is excluded from that agreement (so it
  never hides an otherwise-healthy console and API), but a genuine
  disagreement between two active workloads demotes the whole component back
  to `Progressing` with no version at all. Per ADR 0022: "Mixed releases,
  failures, missing evidence and incomplete rollouts cannot become a single
  healthy running version."
- **Every observation is a timestamped sample.** `DeploymentObservation`
  always carries `observed_at_unix_seconds` from the read that produced it —
  never a cached earlier success. `KubernetesObserver::observe` bounds the
  whole per-component read at 5 seconds (`tokio::time::timeout`); a timeout
  produces an `Unavailable` observation with an explanatory `detail`, never
  the last thing that worked.

## How the pieces fit

```text
fabric_platform_management::DeploymentObserver   the port
        |
   observer.rs   KubernetesObserver::observe(environment, component)
        |         -> looks up this component's WorkloadTargets, bounds the read at 5s
        |
   observer.rs   workload(target): Deployment (before) -> ReplicaSets+Pods (list) -> Deployment (after, must agree)
        |
   evaluate.rs    per-workload health, from owner-verified, digest-verified evidence
        |
   summary.rs     per-component DeploymentObservation, from every workload's health
        |
   client.rs      the one place a request is sent: fresh token, CA-pinned TLS, no redirects, size-bounded
```

## Getting started

```rust,ignore
use std::collections::BTreeMap;
use std::sync::Arc;
use fabric_core::SystemClock;
use fabric_deployment_kubernetes::{KubernetesObserver, WorkloadTarget};

let mut targets = BTreeMap::new();
targets.insert(
    "control-plane".to_owned(),
    vec![WorkloadTarget {
        namespace: "saas-fabric".to_owned(),
        deployment: "control-plane".to_owned(),
        container: "control-plane".to_owned(),
        repository: "ghcr.io/fieldstatenz/saas-fabric-control-plane".to_owned(),
    }],
);

let observer = KubernetesObserver::in_cluster("production", targets, SystemClock::shared())?;

let service = fabric_platform_management::PlatformManagement::new(registry, charts, desired_state, clock)
    .with_observer(Arc::new(observer));
```

## Common tasks

- **Adding a workload to observe** — add an entry to the `environment`'s
  `BTreeMap<String, Vec<WorkloadTarget>>` keyed by the platform component
  name (the same name `fabric-platform-git`'s manifest uses); 1–16 targets
  per component. This is host/deployment configuration, never something an
  API request carries.
- **Debugging `Running: Unknown`** — check, in order: is an observer even
  attached (`PlatformManagement::with_observer`); does `environment` match
  exactly what `KubernetesObserver::in_cluster` was built with; is the
  component name a key in `targets`; and then look at
  `ComponentStatus::observation`'s `detail` field for what the read actually
  found.
- **Granting the RBAC this crate needs** — per ADR 0022: `get` on the named
  `Deployment`s only, and `list` on `Pod`s and `ReplicaSet`s in the
  workloads' namespaces. No `create`, `update`, `delete`, `exec`, secret
  read, or cluster-wide permission. Native RBAC cannot itself restrict the
  `list` permissions to the named Deployment's pods — that narrowing is
  `evaluate.rs`'s owner-chain check, not a cluster policy.

## Gotchas

- The crate is `fabric-deployment-kubernetes` (hyphen); the Rust identifier
  — and the `RUST_LOG` filter target — is `fabric_deployment_kubernetes`
  (underscore): `RUST_LOG=info,fabric_deployment_kubernetes=debug`.
- `KubernetesObserver::in_cluster` is the **only** constructor. There is no
  way to point this crate at anything but `https://kubernetes.default.svc`
  with the pod's own projected CA and token — it is deliberately not general
  Kubernetes-API-client code.
- The bearer token is read from disk on **every** request, never cached.
  This is what lets a projected service-account token's scheduled rotation
  take effect without restarting the process — see `client.rs`'s module
  docs and `credentials_rotate_and_only_get_requests_are_sent` in
  `client_tests.rs`.
- A list response's `metadata.continue` being non-empty is refused
  (`"Deployment evidence requires more than one page."`), never silently
  followed — this crate's `limit=500` query parameter is meant to make one
  page enough, and a second page appearing is treated as evidence the
  request needs revisiting, not as something to paginate through
  transparently.
- Every response body is capped at 2 MiB, enforced as it streams
  (`client.rs::Client::get`), the same "bound the network, not just the
  business object" discipline `fabric-registry`'s chart-index reader uses.
- No error message this crate produces ever contains the cluster's response
  body or a credential — see `errors_do_not_expose_cluster_response_or_credentials`
  in `client_tests.rs`. A `401`/`403` is always "Deployment observation is
  not permitted.", never whatever the API server actually said.
- `WorkloadTarget` derives `Deserialize` with `deny_unknown_fields`; it is
  host configuration parsed once at startup, not a value any handler
  constructs from a request.
- Tests live beside the module they test via `#[path = "..."] mod tests;`
  (`client_tests.rs`, `evaluate_tests.rs`, `summary_tests.rs`) rather than
  inline `#[cfg(test)] mod tests { ... }` blocks — the one place in this
  documentation slice that convention differs from the other four crates.
