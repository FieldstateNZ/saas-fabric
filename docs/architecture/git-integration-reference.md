# Reference: the Workspec Enterprise GitHub App flow

- **Status:** Analysis, preceding implementation.
- **Source read:** `workspec/artifacts/api-server` at `d4a4c1d3`, principally
  `services/github-app-provisioning.ts`, `routes/github-provisioning.ts`,
  `services/github-install.ts`, `services/setup-state.ts`, and
  `workspec/artifacts/workspec/src/pages/setup.tsx`.
- **Why:** the change brief directs that this flow be inspected and reused
  conceptually rather than a second installation protocol invented.

Workspec has **two** GitHub flows and only one of them is the reference here.
The per-workspace consumer install (`routes/github-install.ts`) explicitly
`404`s on Enterprise, because — in its own words — Enterprise "self-provisions
one platform App installed on the platform org". Fabric is the Enterprise
shape: one instance, one App, one org.

## What Workspec does

| Concern | Workspec |
|---|---|
| Entry point | `GET /api/github-install/manifest`, admin-only, returns `{postUrl, manifest}` |
| Redirect / install mechanism | the browser builds a real `<form method=POST>` to `github.com/organizations/{org}/settings/apps/new?state=…` — a manifest hand-off cannot be a `fetch` |
| Callback | `GET /api/github-install/manifest-callback?code&state` → `POST /app-manifests/{code}/conversions` |
| State / correlation | stateless HMAC-SHA256 over `userId.nonce.expiry`, 10-minute TTL, timing-safe compare, **no session cookie** — GitHub's cross-domain redirect will not carry one |
| Identifiers retained | `app_id`, `slug`, `client_id`, `installation_id`, `platform_org` |
| Private credential | `pem`, `client_secret`, `webhook_secret`, AES-256-GCM encrypted into a Postgres `instance_settings` row |
| Reconnect / reinstall | `connect-existing` (paste an App's credentials), plus install auto-discovery via `GET /app/installations` |

Three decisions in it are worth taking wholesale.

**The App is created, not configured.** GitHub's App Manifest flow means no
human ever hand-creates an App or copies a private key: the instance POSTs a
manifest describing the App it wants, and GitHub hands back the identity. This
is the mechanism that removes the human bootstrap dependency, and it is the
reason the brief is achievable at all.

**Setup state is derived from ground truth, never advanced imperatively.**
`deriveSetupState` reads whether the App id exists, then whether the
installation id exists, and returns the state. The stored column is a
materialised cache, reconciled on read. There is no cursor to get out of sync,
so the flow is resumable across restarts and partial setups. Fabric already
reasons this way about reconciliation status.

**An installation is recorded only after a token has been minted for it.**
`persistInstallation` mints an installation token *before* writing the id, and
declines to record it if the mint fails. "Recorded" therefore means "proven",
and no separate verified flag is needed. `clearInstallation` is guarded on the
id matching, so a stale event cannot wipe a good installation.

## Where SaaS Fabric must differ

### 1. No webhooks — the operator plane is a tailnet

Workspec's manifest sets `hook_attributes.url` and depends on `installation`
events to notice an uninstall. Fabric's control plane is published on the
Tailscale operator plane and on **no** public plane, deliberately: it has no
`gateway-access` label and cannot attach to the product `Gateway`. GitHub's
servers cannot reach it.

The redirect legs are unaffected — GitHub redirects the *operator's browser*,
which is on the tailnet, and never fetches those URLs itself. So Fabric takes
the manifest and install round trips and **omits webhooks entirely**
(`hook_attributes.active: false`).

The cost is that Fabric learns about a revoked installation when it next tries
to use it, rather than being told. That is acceptable because Fabric already
polls: reconciliation runs on an interval and reports per-client status. A
revoked installation surfaces as a failing integration on the next sweep. It
does mean integration health must be *probed*, not merely remembered.

### 2. No reachability self-check

Workspec fetches its own `PUBLIC_BASE_URL/api/healthz` before creating the App,
because GitHub starts delivering webhooks immediately. It then had to add
`SETUP_SKIP_REACHABILITY` for precisely our situation — a tailnet ingress where
the pod cannot resolve its own external hostname.

With no webhooks there is nothing to be reachable *for*, so Fabric does not
inherit the check or the bypass flag. What Fabric does need is its own external
URL, to build the manifest's `redirect_url`; that is deployment-supplied
configuration and is validated as a URL, not by dialling it.

### 3. Credentials go to the secret capability, not to an encrypted column

Workspec encrypts secrets with an application-held key into its own database.
Fabric has no database, and the brief is explicit that credential material
belongs to the Fabric instance's secret partition.

Fabric already has the right seam: `SecretResolver` in `fabric-connector`, with
`SecretRef`/`ResolvedSecret` and an environment-backed implementation. It is
**read-only**, which is the gap — establishing an integration means *writing* a
secret. This change adds a write capability behind the same abstraction, so the
GitHub integration domain never names OpenBao, a path, or a mount.

That has a deployment consequence Workspec does not have: a secret projected
into the pod by External Secrets is a one-way delivery, so the control plane
must become a client of the secret store rather than only a reader of its
environment.

### 4. The state signature must bind an operator Fabric can re-check

Workspec signs `userId` and, on callback, re-reads the user to confirm they are
still an active admin — the signature proves the round trip started here, and
the re-check proves the actor is still entitled. Fabric keeps both halves, but
its operator identity is an allowlisted login established by the operator-plane
proxy rather than a database row, so the re-check is against the allowlist.

Fabric also treats the state as genuinely single-use. Workspec's
`consumeInstallState` does not consume anything — it verifies a signature
within a TTL, so a captured state is replayable for ten minutes. Fabric records
the nonce until it expires and rejects a second presentation.

### 5. Instance-scoped from the start

Workspec's Enterprise settings are a singleton row. The brief requires the
domain be shaped so tenant instances can hold their own Git integrations later,
even though only the master instance needs one now. Fabric keys integration
state by instance rather than assuming one, and implements the master instance
only.

### 6. Deliberately not taken

- **`connect-existing`.** A second path that reintroduces pasting a private key
  into a browser form — the exact posture this change removes. Recovery is
  re-running the flow.
- **`administration: write`.** Workspec creates repositories. Fabric's brief
  lists repository creation as a non-goal and its need is `contents: read/write`
  on one repository.
- **OAuth client id/secret.** Workspec collects them and notes they go unused;
  Fabric authenticates as the App with the private key and does not request
  them.
