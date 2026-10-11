# ADR 0027 — Data-path authorization is layered, chosen per tenant, and fails closed

- **Status:** Proposed
- **Date:** 2026-10-11
- **Applies to:** `fabric-data-api` (the `authorization` module and
  `execution::authorize`), a new adapter crate `fabric-fga-check`,
  `fabric-identity` (`extract_bearer` is now public), `fabric-api` (its
  configuration and composition), `scripts/check_architecture.py`, and
  `docs/architecture/crate-dependencies.md`
- **Related:** [ADR 0013](0013-authorization-is-declared-in-the-platforms-words.md);
  [ADR 0016](0016-fabric-owns-the-authorization-front-door.md);
  [ADR 0009](0009-operator-identity-is-not-tenant-identity.md);
  [ADR 0002](0002-trusted-ingress-is-the-canonical-identity-model.md);
  [ADR 0018](0018-runtime-state-is-published-as-three-versioned-documents.md);
  [M0 contract decisions](../roadmap/m0-contract-decisions.md) D01-10, D01-10a
  and D01-12, disposed in
  [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)

## Context

Before this change the Data API decided authorization with
`ResourcePermissions`:

- a scope check, `data:<resource>:<read|write>`;
- `require_scopes = false`, which made every check pass;
- `administrator_role` (default `platform-admin`), a role from the tenant's own
  realm that passed every check on every resource.

Nothing called the authorization front (ADR 0016), so the relations a client
declares under `spec.authorization` (ADR 0013) were enforced nowhere.

The dispositions recorded on #108 settle the shape:

- **D01-10/10a.** Layered authorization. Scopes and roles only for the isolated
  synthetic trial. OpenFGA `Check` on relation-declaring resources from M3, and
  for every tenant beyond the trial. Design and test it now; no wider
  deployment or standing access is authorized.
- **D01-12.** No data-path administrator bypass. Scopes cannot be disabled
  outside tests. Coarse platform-operator controls stay within their current
  boundary.

## Decision

### 1. Two layers, in a fixed order

```text
layer 1  scope            data:<resource>:<read|write> on the token          every request
            │ missing → 403, and layer 2 is never asked
            ▼
layer 2  relation Check   the front's POST /v1/check, as the caller          where the tenant's
            │                                                                policy requires it
            │ denied → 403; no decision → refused (5xx)
            ▼
         permitted
```

Layer 1 runs alone first. A caller without the scope never causes a Check, so
the authorization service's latency and availability cannot be used to learn
which resources declare relations. An allow from layer 2 never stands in for a
missing scope.

Both layers sit at step 2 of `prepare`, ahead of every step that describes the
resource. The ordering rule `tests/authorization_ordering.rs` pins is
unchanged.

### 2. Layer 2 asks the front, as the caller

The Data API asks through a port, `AuthorizationCheck`:

```text
may <the holder of this credential> <operation> <resource>:<key> ?
```

- **The credential is the caller's own bearer token.** ADR 0016 keeps the
  user's JWT as the credential at the Fabric boundary and allows no workload
  identity in the authorization path. The runtime has no credential of its own
  to send, and none is added.
- **The question names an operation, not a relation.** ADR 0013 puts the
  translation into the authorization model in the adapter. The adapter asks
  the computed relation `can_<operation>`, so which declared relations grant
  an operation stays the model's business.
- **The object is `<resource>:<key>`.** The resource is the catalogue's
  `LogicalResourceName`, the spelling desired state declares relations
  against. A key that cannot be written as an object (empty, over 255 bytes,
  or containing `:`, `#`, `/` or whitespace, the front's own rules) is refused
  as a denial, without asking.
- **The adapter speaks to the front, never to OpenFGA.** OpenFGA has one door
  (ADR 0016). The adapter sends a relation and an object; the front takes the
  subject, the tenant, the store and the model from the verified token.

The adapter is `fabric-fga-check::FrontCheck`. It does not follow redirects or
use a proxy from the environment, because either would carry the caller's
token to an address nobody configured.

### 3. The policy is per tenant, and the trial must be named

`TenantPolicies` maps a tenant to a `TenantAuthorization`, which holds two
independent facts:

| Fact | Owned by | Values |
|---|---|---|
| Posture | the launch decision | `SyntheticTrial` or `RelationChecked` |
| Resources that declare relations | the client's `spec.authorization` (ADR 0013) | a set of resource names |

Layer 2 is required when the posture is `RelationChecked` **and** the resource
declares relations. Otherwise layer 1 decides alone.

There is no global switch. A tenant with no entry is beyond the trial. It has
no declarations this process knows of, so it is held to its scopes. Once
declarations exist for it they are enforced, with no posture to remember to
change. The trial is the exception, so the trial is what has to be named.

### 4. Failure is fail closed

| Outcome | Answer | Code |
|---|---|---|
| Scope missing | 403 | `forbidden` |
| Check denied | 403 | `forbidden` |
| Check required, the operation names no record | 403 | `forbidden` |
| Check required, the key cannot name an object | 403 | `forbidden` |
| Front unreachable, timed out, or `5xx` | 503, `Retry-After` | `authorization_unavailable` |
| Front answered something that is not a decision (`401`, `4xx`, a redirect, a malformed body) | 500 | `internal` |
| Check required, no front configured | 500 | `internal` |

Every denial answers the same body, so a status cannot say which layer refused.
The log line `data_api.operation_forbidden` carries `reason`. No failure to
decide is ever turned into an allow. It is not turned into a denial either,
because a denial would tell the caller their access is wrong when the platform
is.

### 5. No administrator bypass, and no way to switch scopes off (D01-12)

- `ResourcePermissions`, `require_scopes` and the Data API's
  `administrator_role` are removed. No role is consulted on the data path.
- No configuration, flag or constructor skips layer 1, in test builds or
  otherwise. A configuration file or `FABRIC_SETTING_PERMISSIONS__*` variable
  that still names `[permissions]` is an unknown key, and the process refuses
  to start.
- The tests that used to switch scopes off now carry scopes. The Data API's
  own suites receive a full-catalogue grant from their shared token builder
  unless a test names `scope` itself.
- **Retained operator control:** the health probes' detail body. Its role moves
  from `permissions.administrator_role` to a top-level `health_detail_role`
  with the same default (`platform-admin`) and the same refusal of a blank
  value. It opens that body and grants nothing on the data path. Operator
  authority on the control plane (ADR 0009) is unchanged.

### 6. Nothing is wired

`fabric-api` composes `DataAuthorization::default()`. That holds no tenant
policies, and its Check is `NoCheckConfigured`, which answers every question
with "not configured". With no policies, nothing reaches layer 2, and every
tenant, the synthetic trial included, is held to its scopes alone, as before.

One behaviour does change: a token carrying `platform-admin` and no scope used
to pass every Data API check and is now refused. That is D01-12's intent.

`fabric-api` does not depend on `fabric-fga-check`. No front endpoint, no
OpenFGA deployment, no credential and no live role changes are part of this
decision.

## Not decided here

- **Collection operations on a relation-declaring resource.** `list` and
  `create` name no record, so there is no object to ask about. For a tenant
  beyond the trial they are refused (the third 403 row above) until a
  collection-level question is defined. The options are on #108.
- **The transport between the runtime and the front.** `FrontEndpoint`
  accepts `http` and `https`. Whether the hop that carries the caller's token
  must be TLS is decided when the endpoint is wired. The question is on #108.

## Owed (not yet built)

Accepting this decision does not make these true.

1. **Where the policy comes from.** Only the composition root can supply
   `TenantPolicies`, and it supplies none. Before M3, the posture and each
   client's relation-declaring resources must reach the runtime through
   publication (ADR 0018), not hand-written runtime configuration.
2. **OpenFGA deployed and models converged (D01-10a, M3).** This needs the
   front's control-plane surface (ADR 0016) and a reconciler that writes
   `spec.authorization` as a pinned model.
3. **The front wired into `fabric-api`.** That means a configured
   `FrontEndpoint` and the front's issuer registry agreeing with the runtime's
   (ADR 0019 §G4a). Until then a `401` from the front is reported as `internal`.
4. **The two-tenant proof (#116)** against the enforcement tenant launch will
   use.

## Consequences

- **Every relation-checked request costs one Check round trip.** Check runs
  before the `405` and placement steps so nothing about the resource leaks.
  It therefore also runs for a verb the catalogue does not expose.
- **A front outage refuses checked requests.** It does not affect the trial or
  any resource that declares no relations. That is the trade D01-10 makes.
- **`CheckObject`'s identifier rules repeat `fabric-fga-auth`'s `ObjectRef`.**
  The Data API has no edge to the trust boundary
  ([crate dependencies](../architecture/crate-dependencies.md)), and the front
  checks again. The copy only decides where an unusable key is refused.

## Evidence

- `crates/fabric-data-api/tests/layered_authorization.rs`, through the real
  router with a scripted Check:
  - `a_trial_tenant_is_allowed_by_scope_alone`
  - `a_tenant_beyond_the_trial_needs_a_check_allow_about_the_record`
  - `a_check_deny_is_refused_like_any_other_denial`
  - `an_unreachable_check_is_refused`
  - `a_check_that_answers_nonsense_or_does_not_exist_is_refused`
  - `a_resource_that_declares_no_relations_uses_scopes_only`
  - `a_missing_scope_is_refused_before_the_check_is_asked`
  - `administrator_and_operator_roles_get_no_data_path_bypass`
  - `a_collection_operation_on_a_checked_resource_is_refused_until_one_is_defined`
  - `a_check_deny_and_a_missing_scope_answer_identically`
- `crates/fabric-data-api/src/authorization/data_authorization/data_authorization_tests.rs`:
  layer order and every failure refusing, without HTTP.
- `crates/fabric-fga-check/tests/front_check_over_loopback.rs`, against a fake
  front on `127.0.0.1`:
  - the caller's token and `can_<operation>` go out unchanged;
  - `{"allowed": false}` is a denial;
  - `503` and an unreachable front are `Unavailable`;
  - `401`, `400`, `403`, a redirect and malformed bodies are `Internal`.
- `crates/fabric-api/src/config/loading.rs`:
  - `scope_checks_cannot_be_switched_off_by_configuration`
  - `a_data_path_administrator_role_cannot_be_configured`
