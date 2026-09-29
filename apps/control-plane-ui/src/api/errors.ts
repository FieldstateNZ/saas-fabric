/** What the control-plane API says when it refuses. */

/** The machine-readable codes the API documents. */
export type ErrorCode =
  | 'unauthenticated'
  | 'operator_refused'
  | 'unknown_client'
  | 'invalid_request'
  | 'desired_state_invalid'
  | 'revision_required'
  | 'revision_conflict'
  | 'realm_immutable'
  | 'repository_unavailable'
  | 'repository_denied'
  | 'repository_rejected'
  | 'platform_not_managed'
  | 'invalid_data_source'
  | 'placement_refused'
  | 'data_source_in_use'
  | 'publication_not_configured'
  | 'publication_running'
  | RegistryErrorCode
  | SelectionErrorCode

/**
 * The codes a refused component selection carries (ADR 0026 section 7),
 * beside the registry's own `registry_unavailable` and `registry_refused`
 * for a registry that could not be asked or refused the request.
 */
export type SelectionErrorCode =
  | 'repository_not_registered'
  | 'component_version_not_found'
  | 'component_version_unusable'
  | 'component_version_already_selected'
  | 'capability_not_selectable'

/**
 * What the rule answered for a version that is not a release unit, carried
 * beside `component_version_unusable`'s code; `reason` only for `invalid`,
 * as `InvalidReason::code` spells it.
 */
export interface SelectionAnswer {
  readonly answer: 'undescribed' | 'incoherent' | 'invalid'
  readonly reason: string | null
}

/**
 * The codes an image registry's refusal carries (ADR 0026 section 5) --
 * never the platform's, because registering a registry is not Platform
 * Management failing. The registries section leads each message with what
 * its code means; the message itself is always the control plane's.
 */
export type RegistryErrorCode =
  | 'registry_not_found'
  | 'registry_exists'
  | 'registry_invalid'
  | 'registry_endpoint_differs'
  | 'registry_credential_unreadable'
  | 'registry_not_proven'
  | 'repository_not_readable'
  | 'registry_refused'
  | 'registry_unavailable'
  | 'registries_unavailable'
  | 'registries_invalid'

/**
 * A refusal from the control plane.
 *
 * Carries the API's own `code` rather than only its message, because the
 * console branches on one case and not on the others: a `revision_conflict`
 * means somebody else edited this client, and the right response is to re-read
 * and tell the operator, not to show a generic failure. Branching on message
 * text would break the moment a message was reworded.
 */
export class ControlPlaneError extends Error {
  readonly code: string
  readonly status: number
  /** Present only on a refused selection whose version the rule answered for. */
  readonly answer: SelectionAnswer | null

  constructor(status: number, code: string, message: string, answer: SelectionAnswer | null = null) {
    super(message)
    this.name = 'ControlPlaneError'
    this.code = code
    this.status = status
    this.answer = answer
  }

  /** Whether the client changed between being read and being written. */
  get isConflict(): boolean {
    return this.code === 'revision_conflict'
  }
}

/** Whether a value is a refusal from the control plane. */
export function isControlPlaneError(value: unknown): value is ControlPlaneError {
  return value instanceof ControlPlaneError
}

/** The answers the selection rule gives for a version it did not call complete. */
const ANSWERS: readonly SelectionAnswer['answer'][] = ['undescribed', 'incoherent', 'invalid']

/**
 * The refusal an API error body describes, or `null` when the body is not
 * one. The body's `answer` and `reason` are kept only when the answer is one
 * this console words: an answer it has never heard of is not given another's
 * wording.
 */
export function refusalFromBody(status: number, body: unknown): ControlPlaneError | null {
  if (typeof body !== 'object' || body === null) {
    return null
  }
  const { error } = body as {
    error?: { code?: string; message?: string; answer?: unknown; reason?: unknown }
  }
  if (!error?.code || !error.message) {
    return null
  }
  const answer = ANSWERS.find((known) => known === error.answer)
  const reason = typeof error.reason === 'string' ? error.reason : null
  return new ControlPlaneError(
    status,
    error.code,
    error.message,
    answer === undefined ? null : { answer, reason },
  )
}
