/** What the control-plane API says when it refuses. */

/** The machine-readable codes the API documents. */
export type ErrorCode =
  | 'unauthenticated'
  | 'unknown_client'
  | 'invalid_request'
  | 'desired_state_invalid'
  | 'revision_required'
  | 'revision_conflict'
  | 'realm_immutable'
  | 'repository_unavailable'
  | 'repository_denied'
  | 'repository_rejected'

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

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = 'ControlPlaneError'
    this.code = code
    this.status = status
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
