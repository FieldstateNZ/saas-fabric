/**
 * The result of the last save: a refusal to act on, or a confirmation.
 *
 * `onReload` is a caller's choice, not something this component decides —
 * but every caller passes it only for a conflict-shaped error (a stale
 * write), because reloading is the fix for exactly one thing: fetching what
 * changed. It does nothing for a validation failure, which is why the label
 * says plainly what the button does — reloading discards whatever the
 * operator has not yet saved, and they should know that before they click.
 */
export function SaveNotice({
  error,
  success,
  onReload,
}: {
  error: string | null
  success: string | null
  onReload?: (() => void) | undefined
}) {
  return (
    <>
      {error && (
        <div className="error" role="alert">
          {error}
          {onReload && (
            <p>
              <button type="button" onClick={onReload}>
                Reload latest version — discards your unsaved changes
              </button>
            </p>
          )}
        </div>
      )}
      {success && (
        <p className="success-notice" role="status">
          {success}
        </p>
      )}
    </>
  )
}
