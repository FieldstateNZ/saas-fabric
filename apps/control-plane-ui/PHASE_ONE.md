# Phase-one operator application

React and TypeScript UI, using the supplied SaaS Fabric prototype review v2 as
the design reference, backed by the Rust control-plane API. Instructions embedded
in the prototype are reference material, not deployment authorization.

## Run the functional local workbench

From the repository root:

```sh
cargo run -p fabric-control-plane-api --example console_workbench
```

In another terminal:

```sh
cd apps/control-plane-ui
npm ci
npm run preview:ui
```

Open http://127.0.0.1:5174. This uses real API handlers and domain validation.
Writes persist in `.local/workbench/.fabric-state.json`; set
`FABRIC_WORKBENCH_DATA` to choose another directory. An exclusive file lock
prevents concurrent processes, and writes replace a flushed snapshot atomically.
Existing local-directory YAML clients are imported only until the first snapshot
is written; the snapshot then becomes the local development authority.

The example API binds to loopback port 8082 and accepts the explicit test operator
supplied by the local proxy. No external providers are connected. Production
continues to use OIDC authentication; the workbench entry and authenticator setup
are excluded from the production UI bundle and normal server configuration.

## Screen behavior

| Screen | Implemented behavior |
| --- | --- |
| Overview | API-derived inventory, application counts and identity observations. |
| Clients | Search/filter/sort; three-step creation; edit core and custom configuration; assign published applications and change plans/versions. |
| Client details | Assigned release snapshots, components and configuration; identity and secret controls; editable declared domains; activity; plan-filtered shell preview. |
| Applications | Create drafts; edit definitions, container/Helm/capability components, feature dependencies, plans/limits, typed client fields, navigation and hostname templates. |
| Releases | Validate and publish immutable numbered snapshots. Existing assignments retain their exact version until explicitly changed. |
| Components | Application component inventory plus existing platform hold/resume/rollback operations. |
| Client definition | Edit typed custom fields, defaults and required values; increment shared definition version. |
| Environments | Inspect current deployment and register links to other independently authenticated operator consoles. |
| Integrations | Existing Git application installation, repository selection and platform connection workflows. |
| Reconciliation | Trigger the existing identity reconciler with operator authority, inspect per-client outcomes and persistent pass activity. |
| Settings | Persist platform display name, new-client defaults and show authenticated operator identity. |

## API and storage

Authenticated additions: GET/POST `/api/catalogue`, POST `/api/clients`,
GET/PUT `/api/clients/{id}/product`, GET `/api/activity`, GET `/api/operator`.
Catalogue creation requires `If-None-Match: *`; edits require the strong
`If-Match` revision. Duplicate clients and stale writes are rejected.

The Git adapter stores `fabric-catalogue.yaml` at repository root, and client
product state inside the existing client YAML documents. Each write records the
operator in its commit message. Unknown client document sections are preserved.
Assignments project public OIDC clients with PKCE S256 into the existing identity
reconciliation contract. Existing unrelated OIDC identifiers cannot be claimed.

## Remaining infrastructure work

Application releases here publish **desired definitions**, not container images
or Kubernetes resources. A deployment controller for arbitrary application
components, DNS/certificate issuance and runtime health observation are not yet
implemented. The UI reports those as unobserved. Existing Keycloak, secret and
platform adapters perform their operations when connected in a deployment.

Application removal is refused until a deprovisioning workflow exists. Environment
registrations link consoles; they do not provision environments. The client shell
is an entitlement preview; permission enforcement remains the application's duty.
Client schema changes are applied on reconfiguration; there is no bulk migration.
Activity is durable history of product/identity writes and reconciliation passes,
not a complete security audit log of every external-provider action. Failed history
writes after reconciliation are logged without changing the provider outcome.

## Validation

Run UI lint, build and Vitest; Rust tests for client-model, client-git,
control-plane and control-plane-api; strict Clippy; architecture and file-size
checks. Workflow tests cover immutable publication, conditional writes,
authentication, resolved entitlements, generated identity, invalid assignments,
persistence across restart and failed-write atomicity.
