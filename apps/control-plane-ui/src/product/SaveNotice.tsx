/**
 * The result of the last save: a refusal to act on, or a confirmation.
 *
 * `onReload` only appears on a conflict-shaped error — offering "reload the
 * latest version" beside an error that has nothing to do with a stale read
 * would suggest a fix that does not apply.
 */
export function SaveNotice({
  error,
  success,
  onReload,
}: {
  error: string | null
  success: string | null
  onReload?: () => void
}) {
  return (
    <>
      {error && (
        <div className="error" role="alert">
          {error}
          {onReload && (
            <p>
              <button type="button" onClick={onReload}>
                Reload latest version
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
