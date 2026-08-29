import { useState } from 'react'

import { converge } from '../api/client'
import { describe } from '../hooks/useClients'

/**
 * Asks the platform to converge every client, now.
 *
 * # Why this is a button and not a schedule
 *
 * The platform holds no credential for the identity provider — it acts with
 * the authority of whoever asked (ADR 0012 in the application repository). So
 * there is nobody to act as when nobody is here, and "check for drift" became
 * something an operator does rather than something that happens.
 *
 * Which is also when somebody is available to act on what it finds.
 */
export function ConvergeButton() {
  const [busy, setBusy] = useState(false)
  const [outcome, setOutcome] = useState<string | null>(null)

  async function run(): Promise<void> {
    setBusy(true)
    setOutcome(null)

    try {
      const { clients } = await converge()
      setOutcome(`Checked ${String(clients)} client${clients === 1 ? '' : 's'}.`)
    } catch (thrown: unknown) {
      setOutcome(describe(thrown))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="converge">
      <button type="button" className="converge__button" disabled={busy} onClick={() => void run()}>
        {busy ? 'Checking…' : 'Check for drift'}
      </button>
      {outcome !== null && <p className="converge__outcome">{outcome}</p>}
    </div>
  )
}
