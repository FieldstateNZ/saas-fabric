import { useState } from 'react'

import { pauseComponent, resumeComponent } from '../api/platform'
import type { PlatformComponent } from '../api/types'
import { describe } from '../hooks/useClients'

/**
 * Stopping a component advancing, and letting it go again.
 *
 * # Why this exists at all
 *
 * Automatic promotion without an in-product pause is an incomplete operator
 * experience. Hand-editing the platform repository still works and remains the
 * break-glass path; the console is meant to be the normal one.
 *
 * # Pause is not a policy change
 *
 * Pausing writes a hold and leaves `update: automatic` alone, so the effective
 * state reads `Automatic — Paused`. An operator who paused before testing a
 * preview did not decide the component should stop advancing forever, and the
 * control offered must not quietly say they did.
 *
 * # Resume permits, it does not advance
 *
 * Lifting a hold says "you may move again". What happens next is the next
 * sweep's to decide, from what it observes then — so this reloads rather than
 * rendering a version nothing has actually moved to.
 */
interface ComponentBrakeProps {
  readonly component: PlatformComponent
}

export function ComponentBrake({ component }: ComponentBrakeProps) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [note, setNote] = useState('')
  const [pausing, setPausing] = useState(false)

  // Nothing to pause. A component that does not advance on its own has no
  // advancement to stop, and the control plane refuses it — so the console
  // does not offer a button whose only outcome is that refusal.
  if (component.policy !== 'automatic') {
    return null
  }

  async function act(work: Promise<void>): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      await work
      // What the environment is now is the platform's to say, not this
      // component's to guess.
      window.location.reload()
    } catch (thrown: unknown) {
      setError(describe(thrown))
      setBusy(false)
    }
  }

  if (component.paused) {
    return (
      <div className="brake">
        {error !== null && <p className="error">{error}</p>}
        <button
          type="button"
          className="brake__action"
          disabled={busy}
          onClick={() => void act(resumeComponent(component.component))}
        >
          {busy ? 'Resuming…' : 'Resume automatic updates'}
        </button>
      </div>
    )
  }

  return (
    <div className="brake">
      {error !== null && <p className="error">{error}</p>}

      {pausing ? (
        <>
          <label className="brake__label" htmlFor={`note-${component.component}`}>
            Why (optional)
          </label>
          <input
            id={`note-${component.component}`}
            className="brake__note"
            value={note}
            onChange={(event) => {
              setNote(event.target.value)
            }}
            placeholder="testing preview.4 by hand"
            disabled={busy}
          />
          <button
            type="button"
            className="brake__action"
            disabled={busy}
            onClick={() =>
              void act(pauseComponent(component.component, note.trim() === '' ? null : note.trim()))
            }
          >
            {busy ? 'Pausing…' : 'Pause'}
          </button>
        </>
      ) : (
        <button
          type="button"
          className="brake__action"
          onClick={() => {
            setPausing(true)
          }}
        >
          Pause automatic updates
        </button>
      )}
    </div>
  )
}
