import { useState } from 'react'

import { pauseComponent, resumeComponent } from '../api/platform'
import type { PlatformComponent } from '../api/types'
import { describe } from '../hooks/useClients'
import { RollbackPicker } from './RollbackPicker'

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
 *
 * # Rollback is offered to every component; pause only to one that moves
 *
 * A component on `manual` or `locked` has no advancement to stop, and the
 * control plane refuses a pause for it — so no pause button. It can still be
 * put back on a version it ran before, and that is exactly the component an
 * operator is most likely to need it for: the platform's own guidance is that
 * a stable component stays `manual` until an upgrade policy exists.
 */
interface ComponentBrakeProps {
  readonly component: PlatformComponent
}

export function ComponentBrake({ component }: ComponentBrakeProps) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [note, setNote] = useState('')
  const [pausing, setPausing] = useState(false)
  const [rollingBack, setRollingBack] = useState(false)

  const advances = component.policy === 'automatic'

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

  if (rollingBack) {
    return (
      <RollbackPicker
        component={component.component}
        artifact={component.artifact}
        onCancel={() => {
          setRollingBack(false)
        }}
      />
    )
  }

  /* Rollback first: it is what an operator reaches for when something is
     wrong, and pausing is what they reach for when they want a moment. It is
     offered whatever the policy and whether or not the component is paused —
     an operator who paused *because* a release broke is the one who needs it —
     and for both artifact kinds: rolling back means restoring a previously
     selected desired version, which a chart supports as much as an image does.
     What differs is how much of the old release comes back, and the picker
     says so in words rather than this hiding the button. */
  const rollBack = (
    <button
      type="button"
      className="brake__action"
      disabled={busy}
      onClick={() => {
        setRollingBack(true)
      }}
    >
      Roll back
    </button>
  )

  if (component.paused) {
    return (
      <div className="brake">
        {error !== null && <p className="error">{error}</p>}
        {rollBack}
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
        <>
          {rollBack}
          {advances && (
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
        </>
      )}
    </div>
  )
}
