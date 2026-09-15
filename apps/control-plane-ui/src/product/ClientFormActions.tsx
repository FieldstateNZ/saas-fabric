/**
 * The footer of every step of {@link ClientForm}'s wizard: Back, the primary
 * action, and Cancel.
 *
 * # One submit, and only one, per click
 *
 * The primary action shares a screen position across every step, but not a
 * mechanism. Before the last step it is `type="submit"`, so "Continue" is
 * the form's own `submit` handler — harmless to fire more than once, since
 * advancing to the same step number again is a no-op. On the last step it
 * becomes `type="button"`, because creating or saving is not harmless to
 * repeat: it is a `POST` or `PUT` that cannot be undone, so it is wired to
 * this button's own `onClick` instead, and refused twice over.
 *
 * First, any click whose `detail` says it is not the first of a sequence is
 * ignored. A double-click's second event can land on this exact button
 * after the first click has already re-rendered it here from "Continue"
 * into its final-step role — `detail` counts by screen position, not by
 * which element received each click, so it still reports the second event
 * as part of the same gesture. Keyboard activation always reports
 * `detail === 0` and is unaffected. Second, `onSubmitFinal` itself is
 * expected to guard the actual request with something React's `disabled`
 * cannot: React does not disable this button synchronously with the click
 * that should have triggered the disable, so `busy` alone would arrive too
 * late to stop a second click landing before the first render commits — see
 * `ClientForm`'s `submitting` ref.
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
      <button
        type={step < 2 ? 'submit' : 'button'}
        className="primary-button"
        onClick={
          step === 2
            ? (event) => {
                if (event.detail > 1) {
                  return
                }
                onSubmitFinal()
              }
            : undefined
        }
      >
        {busy ? 'Saving…' : step < 2 ? 'Continue' : existing ? 'Save client configuration' : 'Create client'}
      </button>
      <button type="button" onClick={onCancel}>
        Cancel
      </button>
    </div>
  )
}
