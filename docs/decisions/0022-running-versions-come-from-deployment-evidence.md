# 0022 — Running versions come from deployment evidence

Status: Accepted for the LucentRoot readiness milestone.

## Context

A desired release in Git does not establish what is serving. The Components
page currently says Running: Unknown even after a successful rollout. Fabric
must be able to demonstrate its own update before deploying application services.

## Decision

Platform Management owns an optional read-only `DeploymentObserver` port. A
Kubernetes adapter implements it; the host supplies the environment and an
explicit component-to-workload binding. The browser cannot supply cluster
addresses, namespaces, selectors, credentials or deployment names. Kubernetes
representations remain inside the adapter, enforced by the architecture gate.
The runtime plane cannot depend on it.

This intentionally introduces a narrowly bounded Kubernetes read capability to
the control-plane service account. The earlier zero-Kubernetes-RBAC deployment
was appropriate before observation existed. No create, update, delete, exec,
secret-read or cluster-wide permission is required. Roles grant get for named
Deployments and list for Pods and ReplicaSets in their specific namespaces.
Native RBAC cannot restrict those list permissions by label; the binding and
owner-chain checks restrict which results contribute evidence.

The adapter rereads the projected account token for each request, verifies the
cluster CA, refuses redirects, caps response sizes and pagination, and bounds
observation time. Failures are current unavailable evidence, never cached success.

A healthy running version requires a current observed controller generation,
converged replica counts, owned ready non-terminating pods and agreement between
the configured container's versioned digest pin and its running image ID. A
second Deployment read rejects a concurrent change. This is a timestamped
sample, not an atomic Kubernetes snapshot or a guarantee of continued health.
Multi-architecture index digests whose running platform digest differs are not
inferred to match; they remain unconfirmed until digest resolution is supported.

All active workloads must agree on a version. An intentionally scaled-to-zero
runtime is shown as stopped and does not hide the active console and API's
version. Mixed releases, failures, missing evidence and incomplete rollouts
cannot become a single healthy running version. The observer does not govern
updates, alter desired state or claim an end-to-end functional health check.

## Rollout

The configuration binding is optional, preserving old configuration compatibility.
Publish and automatically advance to the observer-capable release first. Only
then add observation configuration and namespaced RBAC through GitOps. Older
binaries reject the new configuration, so rollback across that boundary must
remove it together with reverting the image pins.

LucentRoot proves this path. Production is not enabled by this change. Service
registration, workload deployment and application-shell modules remain subsequent
milestones; observation is the first prerequisite that makes their status honest.
