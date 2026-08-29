import { useEffect, useState } from 'react'

import { chooseRepository, listRepositories } from '../api/client'
import type { Candidate } from '../api/types'
import { describe } from '../hooks/useClients'

/**
 * Choosing which repository holds client configuration.
 *
 * Only rendered when the installation reaches more than one. When it reaches
 * exactly one the platform adopts it without asking, because there is no
 * choice to make and confirming it would be ceremony.
 */
export function RepositoryPicker() {
  const [candidates, setCandidates] = useState<readonly Candidate[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    let current = true

    listRepositories().then(
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
  }, [])

  async function choose(candidate: Candidate): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      await chooseRepository(candidate.owner, candidate.name)
      // The platform binds desired state as part of accepting this, so a
      // reload is what shows the console the clients it can now read.
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
