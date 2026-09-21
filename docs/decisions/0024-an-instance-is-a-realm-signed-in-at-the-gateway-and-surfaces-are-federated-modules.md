# ADR 0024 — An instance is a realm, signed in at the gateway, and product surfaces are federated modules

- **Status:** Accepted (2026-09-22, product owner)
- **Date:** 2026-09-22
- **Applies to:** `apps/app-shell`, `apps/control-plane-ui`,
  `fabric-control-plane` (the operator posture, `/api/session`,
  `/api/operator`), `fabric-control-plane-api`, `hosting/`, the runtime plane
  (a later identity route), and `saas-fabric-platform` (the operator ingress)
- **Related:** [ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md);
  [ADR 0021](0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md);
  [ADR 0023](0023-data-sources-are-environment-desired-state-and-placement-is-recorded.md);
  the reference implementation in Karo's `wellbeing-os` (its client directory,
  `docs/kotahi/REALM_RESOLUTION.md`, and the `master` client entry), which
  this decision adopts rather than re-derives

## Context

Three things exist and do not fit together.

**The app shell** (`apps/app-shell`) is a YAML-driven React shell: branding,
theme, navigation and layout come from one bundled file; its pages are two
registered demo components. It has no sign-in, no notion of who is looking at
it, and no way to load anything it was not built with.

**The console** (`apps/control-plane-ui`) signs an operator in by itself. The
browser generates a PKCE pair, asks the control plane where to go
(`GET /api/session`), navigates to the provider, and hands the code back to
the control plane to redeem (`POST /api/session`); the token lives in memory
for the life of the tab, and a silent re-sign-in hides that on every reload.
`GET /api/operator` returns the subject and nothing else. Six hundred lines of
session code exist to make one surface's sign-in work.

**The identity runtime** (`fabric-identity`) already answers the question the
shell needs answered — which tenant, which subject, which roles and scopes a
verified bearer represents — but server-side, on the Data API path, with no
route a browser could call.

The product owner's direction, on 2026-09-22: *the master realm becomes an
instance just like every other instance; the control-plane instance simply
talks to the master realm rather than a client realm.* On login the user is
presented with the shell; the shell derives who they are from Keycloak and
the identity runtime; and the console becomes a plugin the shell loads.

Karo's Wellbeing OS has this working, and its shape is the one adopted here:

- A **client directory** — one directory per client, and **`master` is one
  of them**: `realm: master`, its own host, the same gateway, the same APIs,
  the same shell, and a navigation config that places the `control-plane`
  module. Nothing about the control plane is a special posture.
- **Sign-in belongs to the gateway.** One Envoy filter chain per client,
  selected by the host; `envoy.filters.http.oauth2` runs the
  authorization-code flow against *that chain's* realm; the session lives in
  HMAC-signed cookies at the gateway; the access token is forwarded upstream
  as a bearer. The browser never holds a token. Karo moved there from a
  BFF-owned session, and documented why: a second session store was a second
  thing to secure and to fail.
- **Every API resolves the realm from the host before it authenticates**, and
  validates the bearer against that realm only. An unresolved host is
  refused, never defaulted — a default realm would serve one client's data on
  another client's hostname.
- **The shell reads identity, it does not derive it.** `/api/user/current`
  projects the verified token's claims — id, username, display name, email,
  and the realm tier as `roles` — and `/api/user/configuration` says which
  sections and applications this user may reach. Keycloak owns who someone
  is; anything finer is a permission check against the module that owns the
  surface.
- **Surfaces are modules**, described by a manifest, placed by configuration,
  loaded by the shell at runtime.

SaaS Fabric already holds most of the principles this needs. ADR 0019 puts
token proof at the edge and lets the issuer name the tenant; the runtime is
authentication-agnostic behind a trusted ingress; `fabric-identity` resolves
identity from a bearer the edge has proven. What it lacks is the two ends:
an edge that signs the browser in, and a shell that reads the result.

## Decision

### 1. An instance is a shell deployment bound to one realm by its host

The gateway resolves the realm from the request host — Envoy Gateway's
`SecurityPolicy` with an OIDC provider on LucentRoot, the Envoy in `hosting/`
locally — runs the authorization-code flow against that realm, holds the
session in gateway cookies, and forwards the access token as `Authorization:
Bearer` to every upstream on that host. The control-plane instance is the one
whose realm is `master`. Nothing the browser carries — no claim, no header,
no query parameter — chooses the realm.

### 2. The browser never holds a token

The shell and every module it loads call their own origin with cookies. A
`401` means "sign in", and the browser navigates to the gateway's sign-in
for that host, which the gateway answers with the realm's login. Consequently
the console's browser-side PKCE, its `sessionStorage` verifier and state, its
in-memory token, its silent renewal, and the control plane's
`GET`/`POST /api/session` retire. The operator posture's bearer verification
— JWKS refresh, issuer and audience checks — stays exactly as it is: the
gateway's forwarded token is what it verifies.

### 3. The shell reads identity from one route, in one shape

`GET /api/user/current` answers from the verified token's claims:

```json
{ "subject": "…", "userName": "…", "displayName": "…", "email": "…", "roles": ["fabric-operator"] }
```

`roles` is the realm tier, not a permission list — for `master` the operator
role; for a client realm the client's realm roles. Anything finer is asked of
the module that owns the surface. On the control-plane instance the control
plane serves it (`/api/operator` keeps answering, subject-only, for one
release). On a client instance the runtime plane serves the same shape from
`fabric-identity`'s `TenantIdentity` — a later slice, but the same route.

### 4. The shell's configuration is served per instance, not bundled

The shell fetches `GET /shell/config.yaml` from its own origin: the
prototype's schema — branding, theme, navigation, layout — plus a `modules`
list. The control-plane instance's deployment supplies the file; a client
instance's is derived from the product catalogue later (ADR 0021 already
describes applications with entry points and navigation). The shell knows no
module at build time.

### 5. A product surface is a federated module

Module Federation 2.0 through `@module-federation/vite`. A module exposes one
entry, `./Module`, a React component taking `{ basePath, user }`; React and
React DOM are shared singletons; the module's `mf-manifest.json` is the
contract the shell loads by URL. The shell routes `#/<module id>/…` to the
module, which owns navigation beneath that prefix and nothing above it.

The console becomes the `control-plane` module: a second build output of its
own image, served under `/modules/control-plane/` by the same ingress that
serves the shell, so the origin is the same and the shell's
`default-src 'self'` policy does not widen. Its API client calls with cookies
and holds nothing. Standalone, it still runs for the console workbench.

### 6. Placement is configuration

The control-plane instance's `config.yaml` places the `control-plane` module
in its navigation, as Karo's `master/nav-config.yaml` does. A client
instance's config places its applications' modules. Adding a surface to an
instance is a configuration change, not a shell release.

## What is built first

In this order, each mergeable on its own:

1. **Gateway sign-in for the control-plane instance.** On LucentRoot, a
   `SecurityPolicy` on the console's route: OIDC against the master realm,
   the access token forwarded. Locally, the `hosting/` Envoy gains the OAuth2
   filter for the control-plane host. The console keeps working behind it
   unchanged; its own sign-in short-circuits when `GET /api/user/current`
   already answers.
2. **The control plane's side.** `GET /api/user/current`; `/api/session`
   retired; the operator posture's docs say the token now arrives from the
   gateway.
3. **The shell.** Served config; sign-in by `401`; the user in the header;
   modules loaded by manifest under their route prefix.
4. **The console as a module.** The federation build, the session-free API
   client, `/modules/control-plane/` in the image and the ingress, the
   `control-plane` entry in the instance's config.
5. **Client instances.** `/v1/identity` on the runtime; shell config derived
   from the catalogue.

## Consequences

### Good

- One sign-in mechanism for every instance, and the control plane is not a
  special case of it.
- No token in the browser, on any instance; no session store in the control
  plane.
- Identity comes from Keycloak through the token the edge proved — the same
  chain ADR 0019 built for tenants.
- Modules are independent of the shell and of each other; the console's
  six hundred lines of session code retire.

### Bad, and accepted

- The gateway is now on the local development path: the shell cannot sign in
  without the `hosting/` Envoy running the flow. That harness exists for this.
- Two hash routers meet; the prefix rule is a convention the shell enforces
  and modules must respect.
- The console's own stylesheet inside an Ant Design shell will clash in
  places. Phase one scopes the module's styles; unification is a design
  round, not a code change.
- `/api/session`'s removal breaks any caller outside the console. None is
  known.

## Alternatives rejected

- **PKCE in the shell.** A token in the browser, on every instance; the
  thing the console's own comments argued against and Karo retired.
- **The control plane as the BFF.** Keeps a session store in the control
  plane and makes the operator instance the one that signs in differently.
- **`iframe` modules.** Isolation the CSP already gives, at the price of
  every shared concern — navigation, identity, theme — being re-plumbed
  through `postMessage`.
- **A separate plugin origin.** Widens the CSP and moves cookies across an
  origin boundary for no gain while the ingress can serve both.

## What this does not decide

Deriving a client instance's shell configuration from the catalogue; module
publication as OCI artifacts alongside login brands; organisations within a
realm; visual unification of the console with the shell.
