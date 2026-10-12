import type { ReaderFinding, ReaderMember, Readers, ReaderState } from '../api/publication-types'
import { Status } from '../console/Status'

/**
 * Which generation the runtime's replicas were found serving (G7), beside
 * the readiness gates, and whether a readiness-gated update is complete.
 *
 * Read-only. Every answer but `applied` says why, and `applied` itself is a
 * timestamped sample: it is never shown as a promise about later requests,
 * and `noReaders` is never shown as withdrawal.
 */
export function ReaderRows({ readers, updateComplete }: { readers: Readers; updateComplete: boolean }) {
  return (
    <dl className="facts">
      <dt>Readers</dt>
      <dd>
        <Status value={STATE[readers.state].pill}>{STATE[readers.state].label}</Status>
        {readers.reasons.length > 0 && ` — ${readers.reasons.join('; ')}`}
      </dd>

      <dt>Expected record</dt>
      <dd>{readers.expected === null ? '—' : stamp(readers.expected.generation, readers.expected.recordSha256)}</dd>

      <dt>Sampled</dt>
      <dd>
        {readers.observedAtUnixSeconds === null
          ? 'Not sampled'
          : new Date(readers.observedAtUnixSeconds * 1000).toLocaleString()}
        {readers.desiredReplicas !== null && ` — ${String(readers.desiredReplicas)} replicas asked for`}
      </dd>

      <dt>Replicas</dt>
      <dd>{readers.members.length === 0 ? 'None' : readers.members.map(member).join('; ')}</dd>

      <dt>Update complete</dt>
      <dd>{updateComplete ? <Status value="applied">Yes</Status> : <Status value="neutral">No</Status>}</dd>
    </dl>
  )
}

const STATE: Record<ReaderState, { pill: string; label: string }> = {
  applied: { pill: 'applied', label: 'Applied' },
  pending: { pill: 'pending', label: 'Pending' },
  unobserved: { pill: 'pending', label: 'Unobserved' },
  noReaders: { pill: 'neutral', label: 'No readers' },
}

const FINDING: Record<ReaderFinding, string> = {
  matches: 'matches',
  behind: 'behind',
  lastKnownGood: 'on its last-known-good copy',
  ahead: 'ahead of the record',
  wrongDigest: 'another record at this generation',
  nothingServed: 'serves no recorded generation',
  failed: 'no answer',
  notReady: 'not ready',
  terminating: 'terminating',
  uncompared: 'not compared',
}

function stamp(generation: number, sha256: string): string {
  return `generation ${String(generation)} (${sha256.slice(0, 12)})`
}

function member(entry: ReaderMember): string {
  const served = entry.served === null ? '' : `, ${stamp(entry.served.generation, entry.served.recordSha256)}`
  const detail = entry.detail === null ? '' : `: ${entry.detail}`
  return `${entry.pod} (${entry.replicaSet}) ${FINDING[entry.finding]}${detail}${served}`
}
