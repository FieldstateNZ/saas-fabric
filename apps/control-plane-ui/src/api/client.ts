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
 * Describes the application to create, and where the browser must post it.
 *
 * Creating one through a manifest needs an HTTP POST of a form field, which a
 * browser can only do by submitting a real form — so the control plane hands
 * back what to post rather than a URL to follow.
 */
export async function beginConnection(
  organisation: string,
): Promise<{ post_url: string; manifest: unknown }> {
  return request('/api/integrations/git/connect', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ organisation }),
  })
}

/** Where the operator installs the application once it exists. */
export async function beginInstall(): Promise<{ url: string }> {
  return request('/api/integrations/git/install')
}

/** Every repository the installation can reach. */
export async function listRepositories(): Promise<readonly Candidate[]> {
  const body = await request<{ repositories: Candidate[] }>('/api/integrations/git/repositories')

  return body.repositories
}

/** Settles on the repository client configuration lives in. */
export async function chooseRepository(owner: string, name: string): Promise<void> {
  await request('/api/integrations/git/repository', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ owner, name }),
  })
}

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

/** Forgets the integration this platform holds. */
export async function disconnectIntegration(): Promise<void> {
  await request('/api/integrations/git', { method: 'DELETE' })
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
async function request<T>(path: string, init?: RequestInit): Promise<T> {
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
