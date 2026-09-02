import { useEffect, useState } from 'react'

import { rollBackComponent, rollbackCandidates } from '../api/platform'
import type { RollbackCandidate } from '../api/types'
import { describe } from '../hooks/useClients'

/**
 * Choosing something this environment ran before.
 *
 * # The list is the only way to answer
 *
 * Every candidate is a version the platform resolved to a complete, coherent
 * release unit — three images that exist and agree about the commit they were
 * built from. There is no field to type a version into, and the control plane
 * refuses one it did not itself observe, so "roll back to whatever Git used to
 * say" is not expressible from here.
 *
 * # Digests are not shown, because they are not chosen
 *
 * An operator picks a version. What gets written is that version, its source
 * commit and its three digests, resolved by the platform at the moment of the
 * write. Showing digests would imply there was something to pick between.
 */
interface RollbackPickerProps {
  readonly component: string
  readonly onCancel: () => void
}

export function RollbackPicker({ component, onCancel }: RollbackPickerProps) {
  const [candidates, setCandidates] = useState<readonly RollbackCandidate[] | null>(null)
  const [more, setMore] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [note, setNote] = useState('')

  useEffect(() => {
    let current = true

    rollbackCandidates(component).then(
      (found) => {
        if (current) {
          setCandidates(found.versions)
          setMore(found.more)
        }
      },
      (thrown: unknown) => {
        if (current) {
          setError(describe(thrown))
        }
      },
    )

    return () => {
      current = false
    }
  }, [component])

  async function choose(version: string): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      await rollBackComponent(component, version, note.trim() === '' ? null : note.trim())
      window.location.reload()
    } catch (thrown: unknown) {
      setError(describe(thrown))
      setBusy(false)
    }
  }

  return (
    <div className="rollback">
      {error !== null && <p className="error">{error}</p>}

      {candidates === null && error === null && <p className="empty">Loading&hellip;</p>}

      {candidates !== null && candidates.length === 0 && (
        <p className="empty">
          This environment has not run an earlier version of {component} that is still published.
        </p>
      )}

      {candidates !== null && candidates.length > 0 && (
        <>
          <label className="brake__label" htmlFor={`rollback-note-${component}`}>
            Why (optional)
          </label>
          <input
            id={`rollback-note-${component}`}
            className="brake__note"
            value={note}
            onChange={(event) => {
              setNote(event.target.value)
            }}
            placeholder="preview.5 broke Secrets"
            disabled={busy}
          />

          <ul className="rollback__list">
            {candidates.map((candidate) => (
              <li key={candidate.version}>
                <button
                  type="button"
                  className="rollback__choice"
                  disabled={busy}
                  onClick={() => void choose(candidate.version)}
                >
                  {candidate.version}
                </button>
                <span className="rollback__revision">
                  built from {candidate.source_revision.slice(0, 7)}
                </span>
              </li>
            ))}
          </ul>

          {/* Said rather than hidden. A list that stopped quietly would read
              as "this is everything there is". */}
          {more && <p className="rollback__more">Older versions exist and are not listed.</p>}
        </>
      )}

      <button type="button" className="brake__action" disabled={busy} onClick={onCancel}>
        Cancel
      </button>
    </div>
  )
}
