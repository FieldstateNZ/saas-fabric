# Phase-one operator application

React and TypeScript UI, using the supplied SaaS Fabric prototype review v2 as
the design reference, backed by the Rust control-plane API. The decisions this
surface makes, and the ones still owed to the product owner, are recorded in
[ADR 0020](../../docs/decisions/0020-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md), which is proposed.

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
refuses a second process, and a write replaces a flushed snapshot by rename; the
directory itself is not flushed.
Existing local-directory YAML clients are imported only until the first snapshot
is written; the snapshot then becomes the local development authority.

**A snapshot from before the catalogue envelope is refused.** The workbench will
not start on a `.fabric-state.json` whose catalogue predates `apiVersion` and
`kind` — including one at `.local/workbench/.fabric-state.json` from an earlier
revision of this work. The error names the file, and there is no legacy read
path. Two ways out, with the workbench stopped:

- **Delete the snapshot.** The workbench's catalogue, and every client created
  through it, are lost. Top-level `*.yaml` clients in the directory are imported
  again on the next start.
- **Wrap the stored catalogue by hand.** In the JSON, `catalogue.text` holds the
  catalogue's YAML. Put `apiVersion: fabric.fieldstate.nz/v1`, `kind: Catalogue`
  and `spec:` on lines of their own before it, and indent the old text two spaces
  under `spec:`. The result must still pass today's catalogue rules — a release
  template with exactly one `{client}` and a public callback, `options` only on
  choice fields, among others — or the workbench refuses it as an invalid
  catalogue instead.

The example API binds to loopback port 8082 and accepts the explicit test operator
supplied by the local proxy, and refuses a request whose `Host` is not
`127.0.0.1:8082` or `localhost:8082`, against DNS rebinding. It reserves no realms
or platform client ids, so creation there refuses only a realm another stored
client already declares. No external providers are connected, so nothing it
accepts can be authorised against an identity provider or converged. Production
continues to use OIDC authentication; the workbench entry and authenticator setup
are excluded from the production UI bundle, normal server configuration and every
image.

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
| Reconciliation | Trigger the existing identity reconciler with operator authority and inspect each client's latest outcome. Passes are observations and are not recorded as activity. In the workbench no identity provider is connected, so a pass is refused as unavailable. |
| Settings | Persist platform display name, new-client defaults and show authenticated operator identity. |

## API and storage

Authenticated additions: GET/POST `/api/catalogue`, POST `/api/clients`,
GET/PUT `/api/clients/{id}/product`, GET `/api/activity`, GET `/api/operator`.
The first catalogue write requires `If-None-Match: *`; every later one, and every
product save, requires the strong `If-Match` revision. Product responses and a
created client carry an `ETag`.

| Answer | When |
| --- | --- |
| `409 client_exists` | creating a client whose id is already a document |
| `409 realm_unavailable` | creating a client whose realm is reserved or already declared by another client; the message names the realm, not the reason |
| `409 revision_conflict` | a stale edit, whose message names the catalogue or the client; or a creation that lost a race on the branch, which can be sent again |
| `413 document_too_large` | a catalogue or client document that would render past 900 KiB |
| `500 desired_state_invalid` | stored data that will not parse — a client's `spec.product`, or the catalogue — which no retry fixes. One unreadable client fails the whole activity listing, and every creation |

The Git adapter stores `fabric-catalogue.yaml` at the repository root, and each
client's product state as `spec.product` inside its existing client document; in
Git, each write's commit message carries a `Requested-by:` trailer. The catalogue
file carries `apiVersion: fabric.fieldstate.nz/v1` and `kind: Catalogue`, with the
catalogue under `spec`, and both are checked before the rest is parsed. The
envelope is storage only; the API's JSON does not include it. Unknown client
document sections are preserved, except that `spec.product` must be exactly the
product shape. Assignments project public OIDC clients with PKCE S256 into the
existing identity reconciliation contract. Existing unrelated OIDC identifiers
cannot be claimed.

## Remaining infrastructure work

Application releases here publish **desired definitions**, not container images
or Kubernetes resources. A deployment controller for arbitrary application
components, DNS/certificate issuance and runtime health observation are not yet
implemented. The UI reports those as unobserved. Existing Keycloak, secret and
platform adapters perform their operations when connected in a deployment.

Application removal is refused until a deprovisioning workflow exists. Environment
registrations link consoles; they do not provision environments. The client shell
is an entitlement preview; permission enforcement remains the application's duty.
Client schema changes are applied on reconfiguration; there is no bulk migration,
and a client keeps its release snapshot until an operator saves it.

Activity is the history of operator-authored product and identity writes, stored
in the same write as the change. Reconciliation passes are not recorded: they are
observations, not desired state. Activity is a view, not the audit trail: client
creation, product saves, identity edits and catalogue commands each also emit a
structured audit event to the log pipeline. Nothing trims activity; the 900 KiB
document limit bounds it by refusal, so once the catalogue or a client document
reaches that size, writes to it are refused until it is trimmed in Git.

A client with a `.internal` or loopback host cannot yet be assigned an
application, because projected identity clients are `claimedHttps` only. An
application's hostname template is checked when it is published, so a template
under `.internal` or `.example.test` never publishes. A product
save rewrites the projected identity clients, so a hand edit to their callbacks
does not survive it.

## Validation

Run UI lint, build and Vitest; Rust tests for client-model, client-git,
control-plane and control-plane-api; strict Clippy; architecture and file-size
checks. Workflow tests cover immutable publication, conditional writes,
authentication, resolved entitlements, generated identity, invalid assignments,
persistence across restart and failed-write atomicity.
