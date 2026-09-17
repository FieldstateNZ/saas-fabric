/**
 * The console's wordmark.
 *
 * One component so the mark is spelled the same way everywhere it appears —
 * the sidebar, the sign-in screen, the loading state in `App.tsx` — rather
 * than each of them hand-rolling the same two `span`s and hoping they stay in
 * sync.
 */
export function Brand() {
  return (
    <span className="fabric-brand">
      <span className="fabric-mark" aria-hidden="true">
        f
      </span>
      SaaS Fabric
    </span>
  )
}
