# control-plane-ui

The SaaS Fabric operator console.

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
  components/   presentation, no fetching
  hooks/        loading and saving
```

No router and no state library. A list and a detail pane is the whole
application, and the first increment of an operator console should be small
enough that its correctness is obvious. It grows a router when there is a second
thing to route to.

## Development identity

In production the operator-plane proxy authenticates the human and states who
they are in a header; the browser never sets it and could not be trusted to
(ADR 0009).

Locally there is no such proxy, so the Vite dev server plays the same role —
[`vite.config.ts`](vite.config.ts) adds the header to proxied requests. That
keeps the application code identical in both environments, rather than growing a
"development mode" that behaves differently from the thing being shipped.

Override with `VITE_DEV_OPERATOR` and `VITE_CONTROL_PLANE`.
