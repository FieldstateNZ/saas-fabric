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

/** One of a client's secrets, as a listing shows it. */
export interface SecretEntry {
  readonly path: string
}

/**
 * What is known about a secret without revealing it.
 *
 * No key names. The store does not return them, so including them would mean
 * reading the secret to draw a list — see the control plane's `SecretMetadata`.
 */
export interface SecretMetadata {
  readonly version: number
  readonly updatedAt: string | null
}

/**
 * A secret's values, held only while the operator is looking at them.
 *
 * Never written to `localStorage` or `sessionStorage`, and never put in a URL.
 * The response that carries these is `no-store`, and keeping a copy anywhere
 * the browser persists would defeat that.
 */
export interface RevealedSecret {
  readonly values: Readonly<Record<string, string>>
}

/**
 * What an environment is asked to run of one component.
 *
 * `running` is always `unknown` today. There is no reconciliation integration,
 * and the platform's desired state having changed is not the same as a
 * rollout having happened — a console that said otherwise would be reporting
 * success from a Git write.
 */
export interface PlatformComponent {
  readonly component: string
  readonly desired: string
  readonly available: string | null
  readonly running: 'unknown'
  readonly policy: 'automatic' | 'manual' | 'locked'
  /**
   * Whether an operator has paused an otherwise automatic component.
   *
   * Beside `policy` rather than a value of it: they did not change what the
   * environment should do in general, and the console must not say they did.
   */
  readonly paused: boolean
  readonly desiredState: 'current' | 'update-available'
  readonly hold: PlatformHold | null
  readonly diagnostics: readonly PlatformDiagnostic[]
}

/** Why advancement is paused. */
export interface PlatformHold {
  readonly reason: string
  readonly since: string
  readonly note: string | null
}

/** A version that exists and was not selected. */
export interface PlatformDiagnostic {
  readonly version: string
  /** `publishing` will resolve itself; `incoherent` will not. */
  readonly state: 'publishing' | 'incoherent'
}

/**
 * What the last reconciliation sweep found.
 *
 * The timestamp is unix seconds, formatted here. A server rendering a time for
 * somebody whose timezone it does not know is a server guessing.
 */
export interface PlatformLastCheck {
  readonly atUnixSeconds: number
  readonly outcome: 'success' | 'failure'
  readonly detail: string | null
}

/**
 * An environment's composition.
 *
 * `lastCheck` is `null` when nothing has checked yet, and that is a real
 * answer rather than a missing one. "Nothing has attempted this" and "this was
 * attempted and found nothing to do" send an operator to different places, and
 * a console that showed them identically would be the reason they could not
 * tell why a published version had not appeared.
 */
export interface Platform {
  readonly environment: string
  readonly components: readonly PlatformComponent[]
  readonly lastCheck: PlatformLastCheck | null
}
