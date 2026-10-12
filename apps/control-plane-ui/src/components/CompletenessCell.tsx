import type { Completeness, CompletenessState } from '../api/publication-types'
import { Status } from '../console/Status'

/**
 * Whether the target held one whole recorded generation at the last
 * inspection, and when that was. Its own row beside `Last pass`, never
 * folded into it: a failed pass can leave the target complete, and only
 * `Complete` lets the documents gate be met.
 */
export function CompletenessCell({ completeness }: { completeness: Completeness }) {
  const { pill, label } = STATE[completeness.state]
  const record = completeness.record
  const at = completeness.observedAtUnixSeconds

  return (
    <>
      <Status value={pill}>{label}</Status>
      {record !== null && ` — generation ${String(record.generation)} (${record.recordSha256.slice(0, 12)})`}
      {at === null ? ' — not inspected yet' : ` — inspected ${new Date(at * 1000).toLocaleString()}`}
      {completeness.reason !== null && ` — ${completeness.reason}`}
    </>
  )
}

const STATE: Record<CompletenessState, { pill: string; label: string }> = {
  complete: { pill: 'applied', label: 'Complete' },
  incomplete: { pill: 'incomplete', label: 'Incomplete' },
  unpublished: { pill: 'neutral', label: 'Unpublished' },
  unobserved: { pill: 'pending', label: 'Unobserved' },
}
