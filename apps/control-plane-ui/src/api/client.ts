/**
 * The console's only way to reach anything.
 *
 * Everything the operator console does goes through these four functions, and
 * they call one origin: the SaaS Fabric control-plane API. There is no code
 * path from this application to an identity provider, to a Git host, or to any
 * other platform service — and no credential for any of them ever reaches the
 * browser.
 *
 * That is not a convention. `scripts/check_architecture.py` fails the build if
 * anything under `src/` names another platform service's API.
 */
import { currentToken, forgetToken } from '../session/session'
import { ControlPlaneError } from './errors'
import type {
  Candidate,
  Client,
  Identity,
  IdentityRequest,
  Integration,
  RevealedSecret,
  SecretEntry,
  SecretMetadata,
} from './types'

/** Every client the platform manages. */
export async function listClients(): Promise<readonly Client[]> {
  const body = await request<{ clients: Client[] }>('/api/clients')

  return body.clients
}

/**
 * Whether the platform can reach client desired state, and where from.
 *
 * Asked before the client list, because "not connected yet" and "the list
 * failed to load" are different things to show an operator and only the
 * platform can tell them apart.
 */
export async function getIntegration(): Promise<Integration> {
  return request<Integration>('/api/integrations/git')
}

/**
 * The connection flow, for one of the two integrations.
 *
 * # Why this is a factory and not a parameter on each call
 *
 * The console mirrors the control plane here. There, which integration a
 * request acts on is decided by the route it was sent to, never by anything in
 * the request; here, a component is handed the endpoints it may use and has no
 * way to reach the other set. A `kind` argument threaded through every call
 * would be one mistyped literal away from connecting the wrong application.
 *
 * The segment is a closed union for the same reason `IntegrationKind` is a
 * closed enum: there are exactly two, they are known when this is compiled,
 * and nothing an operator types reaches this.
 */
export interface IntegrationEndpoints {
  /** Describes the application to create, and where the browser must post it. */
  beginConnection(organisation: string): Promise<{ post_url: string; manifest: unknown }>
  /** Where the operator installs the application once it exists. */
  beginInstall(): Promise<{ url: string }>
  /** Every repository this installation can reach. */
  listRepositories(): Promise<readonly Candidate[]>
  /** Settles on one of them. */
  chooseRepository(owner: string, name: string): Promise<void>
  /** Forgets this integration, and only this one. */
  disconnect(): Promise<void>
}

function endpoints(segment: 'git' | 'platform'): IntegrationEndpoints {
  const base = `/api/integrations/${segment}`

  return {
    // A real form POST, not this call: creating an application through a
    // manifest needs the browser to navigate to GitHub's approval screen, so
    // the control plane hands back what to post rather than a URL to follow.
    async beginConnection(organisation: string) {
      return request(`${base}/connect`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ organisation }),
      })
    },

    async beginInstall() {
      return request(`${base}/install`)
    },

    async listRepositories() {
      const body = await request<{ repositories: Candidate[] }>(`${base}/repositories`)

      return body.repositories
    },

    async chooseRepository(owner: string, name: string) {
      await request(`${base}/repository`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ owner, name }),
      })
    },

    async disconnect() {
      await request(base, { method: 'DELETE' })
    },
  }
}

/** Connecting where client configuration is kept. */
export const clientConfiguration: IntegrationEndpoints = endpoints('git')

/** Connecting where this platform's own composition is kept. */
export const platformManagement: IntegrationEndpoints = endpoints('platform')

/**
 * Converges every client onto desired state, with this operator's authority.
 *
 * There is no background sweep: the platform holds no credential for the
 * identity provider and acts as whoever asked, so "check and converge" is an
 * action rather than a schedule.
 */
export async function converge(): Promise<{ clients: number }> {
  return request('/api/reconciliation', { method: 'POST' })
}

/** One client's identity configuration and reconciliation state. */
export async function getIdentity(clientId: string): Promise<Identity> {
  return request<Identity>(`/api/clients/${encodeURIComponent(clientId)}/identity`)
}

/**
 * Replaces a client's identity configuration.
 *
 * `revision` is the version the operator was editing, sent as `If-Match`. The
 * control plane refuses the write if the client has changed since — so two
 * operators editing at once produce a conflict one of them is told about,
 * rather than a silent lost update.
 *
 * A successful write does **not** mean the identity provider has been changed:
 * the response reports reconciliation as `pending`, and it becomes `applied`
 * when reconciliation has actually converged.
 */
export async function putIdentity(
  clientId: string,
  revision: string,
  identity: IdentityRequest,
): Promise<Identity> {
  return request<Identity>(`/api/clients/${encodeURIComponent(clientId)}/identity`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
      'If-Match': `"${revision}"`,
    },
    body: JSON.stringify(identity),
  })
}

/** Issues a request and turns a refusal into a {@link ControlPlaneError}. */
export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  // A `Headers` rather than an object spread: `HeadersInit` is also allowed to
  // be an array of pairs or a `Headers`, and spreading either of those into an
  // object literal produces a header named `0`.
  const headers = new Headers(init?.headers)
  headers.set('Accept', 'application/json')

  // The operator's own token, when this deployment signs operators in. Under
  // the trusted-header posture there is none and the proxy has already said
  // who is calling, so an absent token is not an error here.
  const token = currentToken()
  if (token !== null) {
    headers.set('Authorization', `Bearer ${token}`)
  }

  const response = await fetch(path, { ...init, headers })

  if (!response.ok) {
    // An expired or rejected token is not a failure of the request the
    // operator made. Forgetting it is what lets the shell notice and offer to
    // sign in again, rather than showing an error on every panel at once.
    if (response.status === 401) {
      forgetToken()
    }

    throw await refusal(response)
  }

  // A 204 has no body. Parsing one would throw on exactly the responses that
  // mean "that worked", which is the least helpful place to fail.
  if (response.status === 204) {
    return undefined as T
  }

  return (await response.json()) as T
}

/**
 * Builds the error for a non-success response.
 *
 * The API's error body is the source of the code and message. A response that
 * is not one — a proxy's HTML error page, say — still has to produce something
 * an operator can act on, so the status is used instead of letting a JSON
 * parse failure surface as an unrelated exception.
 */
async function refusal(response: Response): Promise<ControlPlaneError> {
  try {
    const body: unknown = await response.json()
    const error = (body as { error?: { code?: string; message?: string } }).error

    if (error?.code && error.message) {
      return new ControlPlaneError(response.status, error.code, error.message)
    }
  } catch {
    // Falls through to the status-only error below.
  }

  return new ControlPlaneError(
    response.status,
    'unexpected_response',
    `The control plane answered ${String(response.status)}.`,
  )
}

/**
 * Where a secret's path goes in a URL.
 *
 * Encoded per segment, so a path keeps its structure while anything unusual
 * inside a segment is escaped. The control plane validates it again on
 * arrival; this is about building a correct URL, not about trust.
 */
function entry(client: string, path: string): string {
  const encoded = path.split('/').map(encodeURIComponent).join('/')

  return `/api/clients/${encodeURIComponent(client)}/secrets/entry/${encoded}`
}

/** Every secret this client has. */
export async function listSecrets(client: string): Promise<readonly SecretEntry[]> {
  return request<SecretEntry[]>(`/api/clients/${encodeURIComponent(client)}/secrets`)
}

/** What is known about one secret, without revealing it. */
export async function secretMetadata(client: string, path: string): Promise<SecretMetadata> {
  return request<SecretMetadata>(entry(client, path))
}

/**
 * Fetches a secret's values, because the operator asked.
 *
 * A `POST` with the path in the body rather than a `GET`: revealing is an act,
 * and a URL would carry it into history, referrers and proxy logs.
 */
export async function revealSecret(client: string, path: string): Promise<RevealedSecret> {
  return request<RevealedSecret>(`/api/clients/${encodeURIComponent(client)}/secrets/reveal`, {
    method: 'POST',
    body: JSON.stringify({ path }),
  })
}

/**
 * Writes a secret against the version the operator was looking at.
 *
 * `expectedVersion` absent means "I believe this does not exist yet". There is
 * no way to spell "overwrite whatever is there", deliberately.
 */
export async function writeSecret(
  client: string,
  path: string,
  values: Record<string, string>,
  expectedVersion: number | null,
): Promise<{ version: number }> {
  return request<{ version: number }>(entry(client, path), {
    method: 'PUT',
    body: JSON.stringify({ values, expectedVersion }),
  })
}

/** Removes a secret and every version of it. */
export async function deleteSecret(client: string, path: string): Promise<void> {
  await request<undefined>(entry(client, path), { method: 'DELETE' })
}
