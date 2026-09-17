/**
 * The footer of every step of {@link ClientForm}'s wizard: Back, the primary
 * action, and Cancel.
 *
 * # A new element for every step, not just the last one
 *
 * Continue is keyed `` `continue-${step}` ``, and the final action is keyed
 * `"final"` — every step transition changes the key, not only the one into
 * the final step. That is load-bearing, not cosmetic: React reuses a DOM
 * node across a re-render when its key does not change, and a node that had
 * keyboard focus keeps it across that reuse. A key shared between two steps
 * would let Enter, pressed twice or held, carry the focused node's activation
 * straight through the step in between — the keyboard equivalent of skipping
 * a page never rendered long enough to read. Changing the key on every
 * transition forces React to unmount the old node and mount a new one each
 * time, and an unmounted node cannot still be focused — the browser moves
 * focus away from it as part of removing it, before the next keypress is
 * handled. A stray Enter after any transition activates nothing.
 *
 * # Two separate defences against a double-click
 *
 * Continue's own click handler calls `event.preventDefault()` when `detail`
 * is greater than one, so the second click of a double-click at an early
 * step is refused before it can submit the form and advance twice. The final
 * action's click handler does the same thing by simply returning instead —
 * it is not a submit button, so there is no default submission to prevent.
 * Keyboard activation always reports `detail === 0` in both handlers, so
 * neither check does anything for a keyboard repeat; that is what the key
 * change above is for.
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
          key={`continue-${String(step)}`}
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
