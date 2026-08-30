# Fabric Console v0 — the screen contract

Not a design. A list of what each screen shows, where each value comes from,
and which values do not exist yet. Ugly and information-dense is the intent:
open Acme and immediately answer *what exists, what is configured, what is
healthy, what can I manage.*

## The shape

```text
Clients
  Acme        connected   acme    identity            healthy
  Contoso     pending     contoso identity            unknown

Clients / Acme
  Overview | Secrets | Authorization | Identity | Modules | Config | Health
```

## Status of every field, honestly

`live` renders from an API today. `hole` needs one and does not have it. The
holes are the point of this document: they are the product requirements the
UI is meant to reveal, in the order the UI needs them.

### Clients list

| column | source | status |
|---|---|---|
| display name, id | `GET /api/clients` | live |
| realm | `GET /api/clients` | live |
| desired-state status | `GET /api/clients/{id}/identity` → `reconciliation` | live, per client |
| enabled modules | — | **hole** |
| health | — | **hole** |

No **Add client**. Creating a client is a workflow, not a form (ADR 0008), and
inventing one here would design it by accident. The action is absent rather
than disabled-with-a-tooltip; if its absence hurts, that is the requirement.

### Overview

| field | source | status |
|---|---|---|
| display name, id, domains, realm | `GET /api/clients` | live |
| desired-state status, last observed, detail | `identity.reconciliation` | live |
| declared roles, application clients | `GET /api/clients/{id}/identity` | live |
| declared authorization resources and relations | client document (ADR 0013) | **hole** — parsed but not served |
| issuer | derivable from realm + platform config | **hole** |
| OpenBao namespace | — | **hole** |
| OpenFGA store and model id | — | **hole** |
| database / FHIR endpoints | — | **hole** |
| module enablement | — | **hole** |
| provisioning / health | — | **hole** |

Holes render as an explicit *not exposed yet* row rather than being omitted.
A screen that silently drops what it cannot show is a screen that lies about
how much of the platform is wired up.

### Secrets — the first interactive tab

Browse the client's partition, reveal deliberately, and write.

- a path tree or flat path list for the client's partition
- keys at a path, **values hidden by default**
- reveal one value on an explicit action, one at a time, never on render
- create, update, delete a secret
- every mutation states what it did; none is silent

**Nothing exists.** `fabric-openbao` reads and writes the *platform's own*
partition, not a client's; there is no client-secret API and no per-client
partition convention. This tab is the reason to build the thinnest one.

Values are never rendered in a list, never logged, and never placed in a URL.

### Authorization — the second interactive tab

Visibility first, management second.

- the client's declared resources, relations and permitted operations
  (client document, ADR 0013)
- the live model in the client's OpenFGA store
- tuples, filterable by user / relation / object
- a **Check** form: subject, relation, object → allowed / not allowed

The declared half needs the client document served. The live half needs a
control-plane path to OpenFGA — which per ADR 0016 means the front's
control-plane surface, and *this* is the operation that finally gives `:8081`
a reason to exist.

### Identity

Visibility now, mutation later.

- realm, declared roles, application clients — **live** (`IdentityPanel`)
- realm users and their role assignments — **hole**

### Modules, Config, Health

Expose what exists before inventing management.

- **Config** — the client's desired-state document as stored. Read-only, and
  honest: this is what Git holds.
- **Modules** — **hole**; no enablement model exists.
- **Health** — reconciliation status is live; everything else is a **hole**.

## Sequencing

```text
Clients shell            → exists
Client Detail shell      → exists, needs tabs
Overview                 → renders live fields, names its holes
Secrets end-to-end       → first real management, first new API
Authorization end-to-end → gives :8081 its first operation
whatever hurts next
```

Each step ends in something an operator can open. A chain of backend PRs that
does not terminate in a visible slice is the failure mode this document exists
to prevent.
