# ADR 0027 — Data-path authorization is layered, chosen per tenant, and fails closed

- **Status:** Proposed
- **Date:** 2026-10-11
- **Applies to:** `fabric-data-api` (the `authorization` module and
  `execution::authorize`), a new adapter crate `fabric-fga-check`,
  `fabric-identity` (`extract_bearer` is now public), `fabric-api` (its
  configuration and composition), `scripts/check_architecture.py`, and
  `docs/architecture/crate-dependencies.md`; from the 2026-10-12 amendment
  (§2a), also `fabric-core` (`CollectionObject`,
  `OperationKind::computed_relation`), `fabric-client-model` (its
  authorization validation) and a new model-writer crate, `fabric-fga-model`
- **Related:** [ADR 0013](0013-authorization-is-declared-in-the-platforms-words.md);
  [ADR 0016](0016-fabric-owns-the-authorization-front-door.md);
  [ADR 0009](0009-operator-identity-is-not-tenant-identity.md);
  [ADR 0002](0002-trusted-ingress-is-the-canonical-identity-model.md);
  [ADR 0018](0018-runtime-state-is-published-as-three-versioned-documents.md);
  [M0 contract decisions](../roadmap/m0-contract-decisions.md) D01-10, D01-10a
  and D01-12, disposed in
  [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415);
  the collection and transport follow-ups, disposed in
  [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6115327492);
  the collection contract's spelling, model writer and model shape, raised in
  [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6118156522)

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
token to an address nobody configured. It reads a decision only from an
HTTP `200`. A `201` or `202` carrying `{"allowed": true}` is not a completed
decision and is refused as `internal`, like every other unexpected status.

### 2a. Collections get their own, tenant-bound Check

*Amended 2026-10-12. Until then this section refused every collection
operation on a checked resource with a 403
(`collection_contract_undefined`), because no model defined the collection
object. The contract below now exists and is covered by tests, so the
refusal is gone (Owed item 1).*

`list` and `create` name no record. They are asked about the resource's
**collection**, a distinct object, never about a record:

- The port's question carries a `CheckTarget`: `Record(resource:key)` for
  `read`, `update` and `delete`, or `Collection` for `list` and `create`.
  Which operations are collection operations is
  `OperationKind::is_collection_operation`, in `fabric-core`, so the runtime
  and the model writer use one partition. The two kinds cannot stand in for
  each other. No record key is invented for a collection, and a record-level
  allow is never borrowed to permit a collection operation.
- **The object is `<resource>_collection:<resource>`**, for example
  `customers_collection:customers`. The spelling is `fabric-core`'s
  `CollectionObject`, beside `OperationKind`, because the control plane writes
  it and the runtime plane asks about it, and the two planes may not depend on
  each other. Its type half must be a `LogicalResourceName`, because the
  front parses it as one. So a resource name longer than 52 bytes has no
  collection, and two declared resources must not collide (`customers` and
  `customers_collection`). `spec.authorization` validation refuses both.
  If one reaches the Data API anyway, it is refused as a denial
  (`unnameable_object`), without asking.
- The collection object is bound to the tenant the same way every Check is.
  The front selects the store and the model from the verified caller token,
  so one tenant's `customers` collection is a different object from
  another's.
- **The model writer defines it.** `fabric-fga-model::authorization_model`
  translates `spec.authorization` into the model. Each declared resource gets
  two types:

  | Type | Direct relations | Computed relations |
  |---|---|---|
  | `customers` (record) | every declared relation | `can_read`, `can_update`, `can_delete` |
  | `customers_collection` | every declared relation | `can_list`, `can_create` |

  - Each `can_<operation>` is the union of the relations whose `permits`
    include the operation.
  - Where no relation permits it, it is still defined, as a relation nobody
    holds (`<first> but not <first>`). A Check therefore answers a denial,
    not a model error.
  - Neither type defines the other's operations, so inside OpenFGA too a
    record can't answer `can_list`, and a collection can't answer
    `can_read`.
  - Nothing links the two types. Holding `viewer` on the collection grants
    nothing on any record, and the reverse holds too.
  - A declared relation may not be named `can_<operation>`, because it would
    then be the computed relation.
- A `can_list` allow authorizes the collection response that model defines.
  It does not filter rows. `ListObjects`, row filtering and their pagination
  are a later refinement.
- `fabric-fga-check` sends a collection question as
  `{"relation": "can_list", "object": "customers_collection:customers"}`. A
  question whose operation and target disagree (`list` about a record,
  `read` about a collection) is refused as `internal`, and nothing is sent.

A missing collection definition, a missing Check, a denial, or an unavailable
or invalid decision never permits a collection operation. Each one is
refused exactly as it would be for a record (§4).

### 2b. The bearer hop is verified TLS

The request to the front carries the caller's bearer token. Rules for that hop:

- **A live endpoint must be `https`.** `FrontEndpoint::parse` refuses `http`,
  whether or not its host is loopback.
- **The client never sends the token over anything else.** It is built with
  `https_only`, and it relaxes no certificate or hostname check. Peers are
  verified against the host's trust roots (ADR 0005). There is no plaintext
  fallback.
- **NetworkPolicy is not a substitute** for transport protection.
- **Plaintext exists only on the test path.** The only plaintext endpoint is
  `FrontEndpoint::loopback_plaintext`, which is compiled into
  `fabric-fga-check`'s own test builds. It refuses any non-loopback address.
  No production code path can construct it.
- The front's current plain-HTTP listener is not evidence that the wiring is
  ready. Certificates, the endpoint and the listener are activation
  prerequisites for a separately reviewed change.

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
| Check required, the key (or, for `list` and `create`, the resource's collection) cannot name an object | 403 | `forbidden` |
| Front unreachable, timed out, or `5xx` | 503, `Retry-After` | `authorization_unavailable` |
| Front answered something that is not a decision: any status but `200` and `5xx` (including `201` and `202`), or a malformed body | 500 | `internal` |
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

## Owed (not yet built)

Accepting this decision does not make these true.

1. ~~**The collection model contract (§2a).**~~ Built 2026-10-12: the
   per-resource collection type and its `can_list` and `can_create`
   relations, in the model writer (`fabric-fga-model`), with tests. The
   refusals in `DataAuthorization::decide` and in `FrontCheck` have given way
   to a collection Check. The written model is proven against a real OpenFGA
   only by a test that is skipped unless one is provided
   (`FABRIC_TEST_OPENFGA_PORT`); CI does not run it.
2. **Where the policy comes from.** Only the composition root can supply
   `TenantPolicies`, and it supplies none. Before M3, the posture and each
   client's relation-declaring resources must reach the runtime through
   publication (ADR 0018), not hand-written runtime configuration.
3. **OpenFGA deployed and models converged (D01-10a, M3).** This needs the
   front's control-plane surface (ADR 0016) and a reconciler that writes
   `spec.authorization` as a pinned model.
4. **The front wired into `fabric-api`.** That needs three things:
   - an `https` `FrontEndpoint`;
   - the front serving verified TLS (§2b);
   - the front's issuer registry agreeing with the runtime's (ADR 0019 §G4a).

   Until then a `401` from the front is reported as `internal`.
5. **The two-tenant proof (#116)** against the enforcement tenant launch will
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
  - `a_tenant_beyond_the_trial_needs_a_check_allow_about_the_collection_to_list_or_create`
  - `a_collection_deny_or_failure_never_reaches_the_connector`
  - `a_check_deny_and_a_missing_scope_answer_identically`
  - `a_collection_deny_and_a_missing_scope_answer_identically`
- `crates/fabric-data-api/src/authorization/data_authorization/data_authorization_tests.rs`,
  without HTTP:
  - the layer order, and every failure refusing;
  - `a_collection_operation_is_asked_about_the_collection_with_the_callers_own_token`;
  - `a_collection_deny_is_refused_like_any_other_denial`;
  - `every_failure_to_decide_about_a_collection_refuses`;
  - `a_resource_whose_collection_cannot_be_named_is_refused_without_asking`;
  - `a_record_allow_is_never_borrowed_for_a_collection_operation`;
  - `a_collection_operation_without_a_configured_check_fails_closed`;
  - `the_trial_and_undeclared_resources_still_list_on_scopes_alone`.
- `crates/fabric-data-api/src/authorization/check_target.rs`: a collection is
  never equal to, and never renders as, a record.
- `crates/fabric-core/src/operation.rs`: only `list` and `create` are
  collection operations; each operation's computed relation is
  `can_<operation>`.
- `crates/fabric-core/src/collection_object.rs`: the collection is
  `<resource>_collection:<resource>`, its type is a valid resource name, a
  resource too long for one is refused, and a collision is detected.
- `crates/fabric-client-model/src/authorization/validation_tests.rs`: a
  resource spelled like another's collection, a resource too long to name its
  collection, and a relation named `can_<operation>` are refused.
- `crates/fabric-fga-model/src/translate/translate_tests.rs`:
  - ADR 0013's example translates to exactly a record type and a collection
    type;
  - neither type defines the other's operations;
  - the collection type is the one the runtime asks about;
  - an operation no relation permits is defined and held by nobody;
  - a declaration validation refuses is not translated.
- `crates/fabric-fga-model/tests/model_against_openfga.rs`, against a real
  OpenFGA, **skipped unless `FABRIC_TEST_OPENFGA_PORT` is set**:
  - the service accepts the model;
  - a collection grant answers `can_list`;
  - a record `owner` grant answers neither `can_list` nor `can_create`;
  - a collection grant answers nothing about a record;
  - a cross-kind question is refused;
  - an operation no relation permits is a denial;
  - a computed relation cannot be written as a tuple.
- `crates/fabric-fga-check/src/front_check/front_check_tests.rs`, against a
  fake front on `127.0.0.1` through the test-only plaintext path:
  - a `200` allow carries the caller's token and `can_<operation>` unchanged;
  - a `200` `{"allowed": false}` is a denial;
  - `201` and `202` with an allow body are `Internal`;
  - `503` and an unreachable front are `Unavailable`;
  - `401`, `400`, `403`, `307` and malformed bodies are `Internal`;
  - each operation asks its own `can_<operation>` about its own kind of
    object, `customers:42` or `customers_collection:customers`;
  - a collection answer is read by the same rules as a record answer;
  - an operation asked about the wrong kind of object is refused without
    sending anything;
  - an `https` endpoint pointed at a plaintext listener sends nothing.
- `crates/fabric-fga-check/src/front_endpoint.rs`:
  - `http` to a non-loopback host is refused;
  - `http` to loopback is refused outside the test path;
  - `https` is accepted and requires TLS;
  - the test path accepts loopback only.
- `crates/fabric-api/src/config/loading.rs`:
  - `scope_checks_cannot_be_switched_off_by_configuration`
  - `a_data_path_administrator_role_cannot_be_configured`
