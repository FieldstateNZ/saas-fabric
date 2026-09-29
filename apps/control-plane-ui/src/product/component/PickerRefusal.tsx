import type { ControlPlaneError } from '../../api/errors'
import { outcomeUnknown, selectionRefusalLead } from './selection-words'

/**
 * A selection that did not land, in plain words ahead of the control
 * plane's own message, which is always shown as it came.
 *
 * Reloading is offered when the catalogue changed underneath the selection,
 * and when no answer came back -- the selection may have been recorded, and
 * only the catalogue as it is now can say. Reloading clears the refusal: it
 * described the catalogue this page read, which is gone.
 */
export function PickerRefusal({
  refusal,
  onReload,
}: {
  readonly refusal: ControlPlaneError
  readonly onReload: () => void
}) {
  return (
    <div className="picker__refusal" role="alert">
      <p>
        <strong>{selectionRefusalLead(refusal)}</strong>
      </p>
      <p>{refusal.message}</p>
      {(refusal.isConflict || outcomeUnknown(refusal)) && (
        <button type="button" onClick={onReload}>
          Reload latest version
        </button>
      )}
    </div>
  )
}
