/**
 * The shapes the control-plane API speaks.
 *
 * Every name here is a SaaS Fabric concept — a client, a realm, a role, an
 * application client, a reconciliation status. None of them is a file, a path,
 * a branch, or the name of a platform service, because none of those appears
 * in the API this console talks to.
 */

/** Where a client's identity stands between what Git says and what exists. */
export type ReconciliationStatus = 'pending' | 'applied' | 'failed' | 'drifted'

/** What is known about whether a client's desired state has taken effect. */
export interface Reconciliation {
  readonly status: ReconciliationStatus
  /** Seconds since the Unix epoch, or null if it has never been established. */
  readonly observedAtUnix: number | null
  /** Why the last attempt failed, if it did. */
  readonly detail: string | null
}

/** One client, as the list and detail views render it. */
export interface Client {
  readonly id: string
  readonly displayName: string
  readonly hosts: readonly string[]
  readonly realm: string
  /**
   * The version of this client's desired state.
   *
   * Opaque: the console compares it and echoes it back, and never parses it.
   * It is what makes a write conditional — see `putIdentity`.
   */
  readonly revision: string
}

/** An application belonging to a client. */
export interface ApplicationClient {
  readonly id: string
  readonly type: 'oidc'
  readonly redirectUris: readonly string[]
}

/** A client's identity configuration, and its reconciliation state. */
export interface Identity {
  readonly realm: string
  readonly roles: readonly string[]
  readonly clients: readonly ApplicationClient[]
  readonly revision: string
  readonly reconciliation: Reconciliation
}

/** The identity an operator submits. A replacement, not a patch. */
export interface IdentityRequest {
  readonly realm: string
  readonly roles: readonly string[]
  readonly clients: readonly ApplicationClient[]
}

/** What state the platform's connection to client desired state is in. */
export type IntegrationStatus = 'not_configured' | 'connected' | 'invalid' | 'error'

/**
 * What the platform reports about that connection.
 *
 * Status, never credentials. There is no token here, no key, and no reference
 * to one — the control plane does not send them and this console could not
 * display them if it wanted to.
 */
export interface Integration {
  readonly status: IntegrationStatus
  readonly connection: string | null
  readonly last_success_at: number | null
  /** Whether this deployment connects its own integration, or states it. */
  readonly managed: boolean
  readonly application: Application | null
}

/** The application the platform created on the Git host. Public facts only. */
export interface Application {
  readonly slug: string
  readonly account: string | null
  readonly installed: boolean
  readonly repository: string | null
}

/** A repository the installation can reach. */
export interface Candidate {
  readonly owner: string
  readonly name: string
  readonly default_branch: string
}
