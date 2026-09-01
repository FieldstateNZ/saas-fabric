import { useEffect, useState } from 'react'

import type { IntegrationEndpoints } from '../api/client'
import type { Candidate } from '../api/types'
import { describe } from '../hooks/useClients'

/**
 * Choosing which repository this integration reads and writes.
 *
 * # The list is the only way to answer
 *
 * Every candidate comes from what *this* application's installation reaches,
 * read from the Git host each time. There is no field to type an owner and a
 * name into, so a repository nobody shared with this installation is not
 * offered — and the control plane refuses one anyway, because a console is not
 * where that rule belongs.
 */
interface RepositoryPickerProps {
  readonly endpoints: IntegrationEndpoints
}

export function RepositoryPicker({ endpoints }: RepositoryPickerProps) {
  const [candidates, setCandidates] = useState<readonly Candidate[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let current = true

    endpoints.listRepositories().then(
      (found) => {
        if (current) {
          setCandidates(found)
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
  }, [endpoints])

  async function choose(candidate: Candidate): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      await endpoints.chooseRepository(candidate.owner, candidate.name)
      // The platform binds as part of accepting this, so a reload is what
      // shows the console what it can now read.
      window.location.reload()
    } catch (thrown: unknown) {
      setError(describe(thrown))
      setBusy(false)
    }
  }

  return (
    <div className="picker">
      {error !== null && <p className="error">{error}</p>}

      {candidates.length === 0 && error === null && <p className="empty">Loading&hellip;</p>}

      <ul className="picker__list">
        {candidates.map((candidate) => (
          <li key={`${candidate.owner}/${candidate.name}`}>
            <button
              type="button"
              className="picker__choice"
              disabled={busy}
              onClick={() => void choose(candidate)}
            >
              {candidate.owner}/{candidate.name}
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
