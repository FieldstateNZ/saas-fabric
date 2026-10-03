/**
 * How the picker words a refused selection (ADR 0026 section 7): what the
 * refusal means, in plain words, ahead of the control plane's own message,
 * which is always shown as it came.
 *
 * The rule's answers reuse the platform diagnostics' words, so a version is
 * called the same thing in the catalogue as on the Components page.
 */
import type { ControlPlaneError, SelectionErrorCode } from '../../api/errors'
import type { InvalidReasonCode } from '../../api/types'
import { REASONS, STATES } from '../../components/diagnostic-words'

/** The codes the picker words for itself. */
type Worded = SelectionErrorCode | 'registry_unavailable' | 'registry_refused' | 'revision_conflict'

const LEADS: Readonly<Record<Worded, string>> = {
  repository_not_registered:
    'This repository is not registered under any registry, so no registry was asked.',
  component_version_not_found: 'The repository has no tag for this version.',
  component_version_unusable: 'This version cannot be selected.',
  component_version_already_selected:
    'This component already records this version’s component descriptor. Nothing changed.',
  capability_not_selectable: 'A platform capability has no version to select.',
  registry_unavailable: 'The registry could not be asked just now. Try again shortly.',
  registry_refused: 'The registry refused the request, or the credential presented to it.',
  revision_conflict: 'The catalogue changed since this page read it.',
}

function isWorded(code: string): code is Worded {
  return Object.hasOwn(LEADS, code)
}

function isReason(code: string): code is InvalidReasonCode {
  return Object.hasOwn(REASONS, code)
}

/**
 * Whether no answer about the selection came back, so whether it was
 * recorded is not known: a request that failed on the network, a body that
 * was not the control plane's (a gateway's, or the request timing out), or
 * a server failure this picker has no words for -- any of which can follow
 * a catalogue write that landed. Every refusal the picker words is decided
 * before anything is written.
 */
export function outcomeUnknown(error: ControlPlaneError): boolean {
  return (
    error.code === 'unexpected' ||
    error.code === 'unexpected_response' ||
    (error.status >= 500 && !isWorded(error.code))
  )
}

/**
 * What a refused selection means. For a version the rule answered for, its
 * answer and, for *invalid*, its reason; a reason this console has no words
 * for is left to the control plane's message rather than given another's.
 * When no answer came back, it says the outcome is not known rather than
 * calling it a refusal.
 */
export function selectionRefusalLead(error: ControlPlaneError): string {
  if (error.answer !== null) {
    const { answer, reason } = error.answer
    if (answer !== 'invalid') {
      return `This version cannot be selected: ${STATES[answer]}.`
    }
    return reason !== null && isReason(reason)
      ? `This version cannot be selected: ${STATES.invalid}, because ${REASONS[reason]}.`
      : `This version cannot be selected: ${STATES.invalid}.`
  }
  if (outcomeUnknown(error)) {
    return 'No answer came back, so whether this version was recorded is not known. Reload to see the catalogue as it is.'
  }
  return isWorded(error.code) ? LEADS[error.code] : 'The control plane refused this selection.'
}
