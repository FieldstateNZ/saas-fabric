import { useEffect, useState } from 'react'

import { rollBackComponent, rollbackCandidates } from '../api/platform'
import type { RollbackCandidate } from '../api/types'
import { describe } from '../hooks/useClients'

/**
 * Choosing a version this environment was previously asked to run.
 *
 * # The list is the only way to answer
 *
 * Every candidate is a version the platform observed just now — for images,
 * one it resolved to a complete, coherent release unit; for a chart, one the
 * repository's index still lists. There is no field to type a version into,
 * and the control plane refuses one it did not itself observe, so "roll back
 * to whatever Git used to say" is not expressible from here.
 *
 * # The two kinds restore different amounts, and it is said rather than hidden
 *
 * An image rollback restores the exact bytes, because a release unit carries
 * every digest. A chart rollback restores the *version*, and a chart
 * repository can republish what sits behind one — so the caveat below is shown
 * for a chart. It is a caveat an operator can act on: it tells them what they
 * are getting, which is more use than the button not being there.
 *
 * # Digests are not shown, because they are not chosen
 *
 * An operator picks a version. What gets written is resolved by the platform
 * at the moment of the write. Showing digests would imply there was something
 * to pick between.
 */
interface RollbackPickerProps {
  readonly component: string
  readonly artifact: 'oci' | 'helm'
  readonly onCancel: () => void
}

export function RollbackPicker({ component, artifact, onCancel }: RollbackPickerProps) {
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
          {/* One line, and only for the kind it is true of. Said before the
              list rather than after it, because it is what the operator needs
              in order to read the list correctly. */}
          {artifact === 'helm' && (
            <p className="rollback__caveat">
              Restores the chart version. A chart repository can republish the bytes behind a
              version, so this is not the byte-for-byte return an image rollback is.
            </p>
          )}

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
                {/* Absent for a chart, because there is no commit to name.
                    Rendering the line empty would say the platform observed a
                    provenance and found nothing in it. */}
                {candidate.source_revision !== undefined && (
                  <span className="rollback__revision">
                    built from {candidate.source_revision.slice(0, 7)}
                  </span>
                )}
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
