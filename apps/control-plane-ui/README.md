# control-plane-ui

The SaaS Fabric operator console.

## Phase-one UI

The console follows the supplied v2 prototype: an overview dashboard, searchable
clients, applications, components and platform navigation. What each screen
does, and what is still missing underneath it, is in [PHASE_ONE.md](PHASE_ONE.md).
The decisions it makes, and the ones still owed, are in
[ADR 0020](../../docs/decisions/0020-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md), which is proposed.

`npm run preview:ui` is **not** a sample-data preview. It serves the console on
`127.0.0.1:5174` and proxies `/api` to the loopback workbench API on
`127.0.0.1:8082`, adding the test-operator header to every request. It needs that
API running, and every save is a real write to the workbench's local storage:

```bash
# Terminal 1, from the repository root — the workbench API.
cargo run -p fabric-control-plane-api --example console_workbench

# Terminal 2 — the console against it.
npm run preview:ui --prefix apps/control-plane-ui
# http://127.0.0.1:5174
```

The sections below describe the API and security contracts that remain in place.

```bash
# Terminal 1 — the control-plane API, with development adapters.
cargo run -p fabric-control-plane-api -- examples/control-plane.toml

# Terminal 2 — the console.
npm install --prefix apps/control-plane-ui
npm run dev --prefix apps/control-plane-ui
```

Then open <http://localhost:5173>.

## What it talks to

The SaaS Fabric control-plane API, and nothing else.

It does not call the identity provider. It does not call the Git host. It holds
no credential for either, and it never receives one — the API it talks to is
carefully built so that no credential can reach a response, and
`scripts/check_architecture.py` fails the build if anything under `src/` so much
as names another platform service's API.

Every request in the console goes through the four functions in
[`src/api/client.ts`](src/api/client.ts), and every one of them uses a relative
path. There is no second origin.

## The vocabulary is the information architecture

**Clients. Identity. Domains.** Not the names of the services that implement
them. An operator manages what SaaS Fabric promises; which platform service
happens to deliver it is not something this console asks them to know, and there
is no control anywhere that opens one.

That is a deliberate constraint, not an aesthetic one — see the platform
specification §16 and §17. The moment the console starts showing realm
representations, it has become an administration front-end for something else
and the abstraction it exists to provide has stopped existing.

## What it shows, and why the badge matters most

```text
Clients
  list

Client detail
  overview      display name, domains, realm
  identity      realm, realm roles, applications, reconciliation
```

The reconciliation badge is the most important thing on the screen. Writing a
document to Git and converging a platform service onto it are **different
events that fail independently** (ADR 0008), so a console that showed only the
desired state would let an operator read a configuration and believe it was
reality.

| Badge | What the operator is told |
|---|---|
| Pending | This configuration has been written but has not taken effect yet. |
| Applied | This configuration is in effect. |
| Failed | This configuration could not be applied. |
| Drifted | Something changed this outside SaaS Fabric. It has been corrected. |

A save answers `pending`, every time, because at that moment it is true.

## What it lets an operator change

**Realm roles.** Add and remove, except the two the platform requires — those
rows have no remove control, because the API refuses it either way and
discovering a rule through an error is worse than seeing it on the row.

**Not the realm.** Moving a client to a different realm would abandon every user
and session in the old one, and the API refuses it. The console says so rather
than offering a field that cannot be saved.

**Not applications, yet.** Shown, because an operator looking at a client's
identity needs to know which applications can sign its users in; not editable in
this increment.

**Product configuration, elsewhere.** Creating a client, its application
assignments and configuration, and the catalogue are edited through the product
workflows in [PHASE_ONE.md](PHASE_ONE.md). None of them edits identity directly:
the API projects each assigned application into the client's identity as a
public client, and a product save rewrites those entries (ADR 0020 §4).

## Concurrency

The console reads a client's identity along with its **revision**, and sends
that revision back as `If-Match` when it saves. If somebody else edited the
client in between, the API refuses the write with a conflict and the console
re-reads rather than retrying — the operator's edit was made against state that
no longer exists, and applying it anyway is the lost update the revision check
exists to prevent.

The revision is opaque. The console compares it and echoes it; it never parses
it.

## Quality checks

```bash
npm run lint       # ESLint, type-aware
npm run typecheck  # tsc -b
npm test           # Vitest
npm run build      # tsc -b && vite build
```

All four run in CI. The lint configuration is the frontend half of this
repository's quality policy: the Rust side denies `unwrap`, `panic` and
indexing, forbids `unsafe`, and fails the build on any production file over 150
lines — this side denies `any`, floating promises, and unused code, and applies
the same 150-line limit with the same exemption for tests.

## Structure

```text
src/
  api/          the only thing that touches the network
  components/   identity, secrets, integration and platform panels
  console/      the shell: navigation, dashboard, client directory, reconciliation
  hooks/        loading and saving for those panels
  product/      the catalogue, application definitions and client workflows
  session/      sign-in: PKCE, the pending session, silent renewal
preview/        the loopback workbench entry and proxy; not in the production build
```

Navigation uses hash URLs without an additional router dependency. The server
continues to serve the root document without a history fallback. Shared console
reads stay mounted while navigating; per-client editors retain their own hooks.

## Development identity

The production console uses OIDC bearer authentication. `npm run dev` proxies
API calls to `VITE_CONTROL_PLANE` (default `http://localhost:8081`).

For a standalone local workbench with persistent storage, see [PHASE_ONE.md](PHASE_ONE.md).
That explicit loopback-only example uses a test operator and has no external
providers, so nothing it accepts can be authorised or converged. It is excluded
from the production entry and from every image. It is also proposed rather than
settled: the control-plane architecture says local development needs a Keycloak,
and [ADR 0020](../../docs/decisions/0020-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md) records the contradiction and leaves keeping the workbench
to the product owner.
