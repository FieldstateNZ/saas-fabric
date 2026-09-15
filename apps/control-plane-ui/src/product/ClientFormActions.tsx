/**
 * The footer of every step of {@link ClientForm}'s wizard: Back, the primary
 * action, and Cancel.
 *
 * # Continue and the final action are two different elements
 *
 * They share a screen position and a label slot, but they are rendered with
 * different `key`s (`"continue"` and `"final"`) rather than one element whose
 * `type` and label change with `step`. That is load-bearing, not cosmetic:
 * React reuses a DOM node across a re-render when its key does not change,
 * which means a node that had keyboard focus keeps it — so a shared node
 * would let three presses of Enter (or one held down) walk Continue through
 * every step and land on the final action while it is still focused, no
 * mouse ever involved. A different key forces React to unmount the old node
 * and mount a new one the moment the role changes, and an unmounted node
 * cannot still be focused — the browser moves focus away from it as part of
 * removing it, before the next keypress is handled. A stray Enter after that
 * activates nothing.
 *
 * # Two separate defences against a double-click
 *
 * Continue's own click handler calls `event.preventDefault()` when `detail`
 * is greater than one, so the second click of a double-click at an early
 * step is refused before it can submit the form and advance twice. The final
 * action's click handler does the same thing by simply returning instead —
 * it is not a submit button, so there is no default submission to prevent.
 * Keyboard activation always reports `detail === 0` in both handlers and is
 * never affected by this check; it is the key change above that stops a
 * keyboard-only repeat.
 */
export function ClientFormActions({
  step,
  busy,
  existing,
  onBack,
  onSubmitFinal,
  onCancel,
}: {
  step: number
  busy: boolean
  existing: boolean
  onBack: () => void
  onSubmitFinal: () => void
  onCancel: () => void
}) {
  return (
    <div className="form-actions">
      {step > 0 && (
        <button type="button" onClick={onBack}>
          Back
        </button>
      )}
      {step < 2 ? (
        <button
          key="continue"
          type="submit"
          className="primary-button"
          onClick={(event) => {
            if (event.detail > 1) {
              event.preventDefault()
            }
          }}
        >
          Continue
        </button>
      ) : (
        <button
          key="final"
          type="button"
          className="primary-button"
          onClick={(event) => {
            if (event.detail > 1) {
              return
            }
            onSubmitFinal()
          }}
        >
          {busy ? 'Saving…' : existing ? 'Save client configuration' : 'Create client'}
        </button>
      )}
      <button type="button" onClick={onCancel}>
        Cancel
      </button>
    </div>
  )
}
