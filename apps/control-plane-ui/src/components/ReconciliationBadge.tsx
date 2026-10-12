import type { HandEdit, Reconciliation } from '../api/types'

/** What each status means, in an operator's words rather than the platform's. */
const EXPLANATION: Record<Reconciliation['status'], string> = {
  pending: 'This configuration has been written but has not taken effect yet.',
  applied: 'This configuration is in effect.',
  failed: 'This configuration could not be applied.',
  drifted: 'Something changed this outside SaaS Fabric. It has been corrected.',
  handEdited:
    'Someone edited this in the identity provider in a way SaaS Fabric does not support. ' +
    'It has been left in place, not overwritten. Undo the edit there, or re-assert the configuration.',
}

/** Each hand edit, as an operator would describe it. */
const HAND_EDIT: Record<HandEdit['field'], string> = {
  clientDisabled: 'was switched off',
  standardFlowDisabled: 'had sign-in (the authorization-code flow) switched off',
  declaredRedirectUriRemoved: 'had a declared callback removed',
}

/**
 * Says whether what an operator is looking at is actually in effect.
 *
 * The most important thing on the screen, and the reason it is a component
 * rather than a line of text: a console that showed only the desired state
 * would let an operator read a document and believe it was reality. Writing to
 * Git and converging a platform service are different events that fail
 * independently, so the console shows both.
 */
export function ReconciliationBadge({ reconciliation }: { reconciliation: Reconciliation }) {
  const { status, observedAtUnix, detail } = reconciliation

  return (
    <div className={`badge badge--${status}`}>
      <div className="badge__row">
        <span className="badge__status">{status}</span>
        <span className="badge__when">{observed(observedAtUnix)}</span>
      </div>
      <p className="badge__explanation">{EXPLANATION[status]}</p>
      {detail !== null && <p className="badge__detail">{detail}</p>}
      {reconciliation.handEdits.length > 0 && (
        <ul className="badge__hand-edits">
          {reconciliation.handEdits.map((edit) => (
            <li key={`${edit.client}:${edit.field}`}>
              <code>{edit.client}</code> {HAND_EDIT[edit.field]}
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/**
 * Formats when the status was last established.
 *
 * The API sends seconds since the Unix epoch and no formatted string, so every
 * opinion about time zones stays in the browser, where the reader is. `null`
 * is rendered as words rather than as a 1970 timestamp, which would read as a
 * bug.
 */
function observed(unixSeconds: number | null): string {
  if (unixSeconds === null) {
    return 'not checked yet'
  }

  return `checked ${new Date(unixSeconds * 1000).toLocaleString()}`
}
