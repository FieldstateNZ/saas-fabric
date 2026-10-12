# Phase-one operator application

React and TypeScript UI, using the supplied SaaS Fabric prototype review v2 as
the design reference, backed by the Rust control-plane API. The decisions this
surface makes, and the ones still owed to the product owner, are recorded in
[ADR 0021](../../docs/decisions/0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md),
which is proposed.

## Running it

See [README.md](README.md), "Running it" and "Development identity". The loopback
workbench that once served this surface with a fixed test operator is retired
(D01-9): the console signs in only through the gateway (ADR 0024).

## Screen behavior

| Screen | Implemented behavior |
| --- | --- |
| Overview | API-derived inventory, application counts and identity observations. |
| Clients | Search/filter/sort; three-step creation; edit core and custom configuration; assign published applications and change plans/versions. |
| Client details | Assigned release snapshots, components and configuration; identity and secret controls; editable declared domains; activity; a Data tab showing each declared logical data source's placement or refusal, and a Place control to request it (ADR 0023 part 2); plan-filtered shell preview. |
| Applications | Create drafts; edit definitions, components — described ones selected from a registered repository's version tags (ADR 0026), and container/Helm/capability ones authored as text — with each described component's resolution and declared fields and resources shown read-only, the logical resources exposed through the Data API (ADR 0023 part 3), feature dependencies, plans/limits, typed client fields, navigation and hostname templates. |
| Releases | Validate and publish immutable numbered snapshots. Existing assignments retain their exact version until explicitly changed. |
| Components | Application component inventory plus existing platform hold/resume/rollback operations. |
| Client definition | Edit typed custom fields, defaults and required values; increment shared definition version. |
| Environments | Inspect current deployment; declare, correct or remove the data sources it can place a tenant on (ADR 0023 part 1, part 2); view the derived runtime catalogue, or the conflict that stops it from being derived (ADR 0023 part 3); register links to other independently authenticated operator consoles. |
| Integrations | Existing Git application installation, repository selection and platform connection workflows. |
| Reconciliation | Trigger the existing identity reconciler with operator authority and inspect each client's latest outcome. Passes are observations and are not recorded as activity. |
| Settings | Persist platform display name, new-client defaults and show authenticated operator identity. |

## API and storage

Authenticated additions: GET/POST `/api/catalogue`, GET `/api/catalogue/runtime`
(the derived runtime catalogue, ADR 0023 part 3), POST `/api/clients`,
GET/PUT `/api/clients/{id}/product`, GET `/api/activity`, GET `/api/operator`,
GET `/api/platform/data-sources`, PUT `/api/platform/data-sources/{dataSourceId}`,
DELETE `/api/platform/data-sources/{dataSourceId}`, GET `/api/clients/{clientId}/placements`,
POST `/api/clients/{clientId}/placements/{logical}` (ADR 0023 part 2).
The first catalogue write requires `If-None-Match: *`; every later one, and every
product save, requires the strong `If-Match` revision. Product responses and a
created client carry an `ETag`. A data source's precondition works differently:
the late-bound binding always has a generation tag to compare-and-swap on, even
before any file exists, so `GET /api/platform/data-sources` always answers a
non-null `revision` and an `ETag`, for an environment that has declared nothing
yet as much as for one that has. `PUT /api/platform/data-sources/{dataSourceId}`
therefore always requires `If-Match`, including the very first declaration --
there is no `If-None-Match: *` case on this route, and a request without
`If-Match` is refused. `DELETE /api/platform/data-sources/{dataSourceId}` requires
the same `If-Match`, against the data-sources revision. `GET /api/clients/{clientId}/placements`
answers the same way, with a non-null `revision` for the placements document even
before this client has ever been placed, and `POST /api/clients/{clientId}/placements/{logical}`
always requires that `If-Match`; there is no request body, because the intent
being placed is the client's own `spec.data.<logical>`, which the control plane
already has.

| Answer | When |
| --- | --- |
| `409 client_exists` | creating a client whose id is already a document |
| `409 realm_unavailable` | creating a client whose realm is reserved or already declared by another client; the message names the realm and which of the two |
| `409 revision_conflict` | a stale edit, whose message names the catalogue or the client; or a creation that lost a race on the branch, which can be sent again. A creation the Git host refused as invalid stays a rejection instead |
| `422 document_too_large` | the document the write would produce is past its limit: 900 KiB for a creation, product save or catalogue command, 960 KiB for an identity edit, so a compromised callback can still be removed from a document growth has filled |
| `422 invalid_data_source` | a data source declaration ADR 0023 part 1 refuses -- a shared placement with no discriminator column, any other placement with one, a zero pool value, an empty label, a default connection kind (a connector's single default connection is never something an operator declares), or a malformed secret reference; the message is the rule's own words |
| `422 placement_refused` | placing a logical data source Fabric cannot place -- no declared data source admits the intent, the logical is already placed, or the client's document declares no data source by that name (ADR 0023 part 2); the message is the refusal's own words |
| `409 data_source_in_use` | removing a data source a held placement still names; the message lists the tenants placed on it |
| `500 desired_state_invalid` | stored data that will not parse — a client's `spec.product`, the catalogue, or an environment's data-sources document (a hand edit gave two entries the same id, or made one invalid) — which no retry fixes. One unreadable client fails the whole activity listing, and every creation. `GET /api/catalogue/runtime` answers it too, for a resource name two different applications declare (ADR 0023 part 3) — reachable only through a hand edit, since publishing itself refuses that; the message names the resource and both applications |

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
structured audit event to the log pipeline. Nothing trims activity; the document
size limits bound it by refusal instead, so once the catalogue or a client
document reaches 900 KiB, writes that grow it are refused until it is trimmed in
Git — an identity edit keeps working up to 960 KiB, so a compromised callback
can still be removed.

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
