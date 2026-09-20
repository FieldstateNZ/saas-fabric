import { useEffect, useState } from 'react'

import { isControlPlaneError } from '../api/errors'
import { publishRuntimeState } from '../api/publication'
import type { PassOutcome, Publication } from '../api/publication-types'
import { Panel } from '../console/Panel'
import { describe } from '../hooks/useClients'
import { capitalise, PublicationRows } from './PublicationRows'

/**
 * What the runtime-publication target holds, what the last pass did, and
 * the one control this slice adds: running a pass now.
 *
 * `publication === null` is a state, not an error: this deployment publishes
 * no runtime state at all (no `[platform_management.publication]`), so it
 * renders as a plain sentence with no button, the way `PlatformNotManaged`
 * does.
 *
 * The row itself never claims the runtime has reloaded — see
 * `PublicationRow`'s own doc comment on the control-plane side. The sentence
 * under the facts list says so, so "published" does not read as "live".
 */
export function PublicationPanel({ publication }: { publication: Publication | null }) {
  if (publication === null) {
    return (
      <Panel title="Runtime publication">
        <div className="panel-body">
          <p className="empty">Runtime publication is not configured for this deployment.</p>
        </div>
      </Panel>
    )
  }

  return <Configured publication={publication} />
}

/**
 * What this panel is telling the operator about its own last action.
 *
 * One value, not an `error` string beside a `refused` boolean: a click ends
 * in exactly one of these, so a union makes "both a fault and a refusal at
 * once" impossible to render, not merely unlikely.
 */
type Notice =
  | { readonly kind: 'ran'; readonly outcome: PassOutcome; readonly detail: string | null }
  | { readonly kind: 'running' }
  | { readonly kind: 'fault'; readonly message: string }
  | null

function Configured({ publication }: { publication: Publication }) {
  const [display, setDisplay] = useState(publication)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<Notice>(null)

  // `usePlatform.refresh` blanks `value` before refetching, so today's
  // **Refresh status** unmounts this panel and remounts a fresh one once the
  // read lands -- `useState`'s own initialiser already seeds `display` for
  // that path, and this effect never fires. It exists for a caller that
  // instead keeps this instance mounted across a refresh: a notice describes
  // the row currently shown, so it must clear the moment that row does.
  useEffect(() => {
    setDisplay(publication)
    setNotice(null)
  }, [publication])

  async function publish(): Promise<void> {
    setBusy(true)
    setNotice(null)

    try {
      // The response *is* the row from the pass this call just ran --
      // rendering it directly is why the trigger answers with the row.
      const row = await publishRuntimeState()
      setDisplay(row)
      // `lastPass` is never actually absent here -- the type just mirrors `GET /api/platform`'s own nullable shape.
      setNotice(row.lastPass === null ? null : { kind: 'ran', outcome: row.lastPass.outcome, detail: row.lastPass.detail })
    } catch (thrown: unknown) {
      if (isControlPlaneError(thrown) && thrown.code === 'publication_running') {
        // A pass already in flight is a refusal of this click, not a fault —
        // there is nothing broken here for `.error` to describe.
        setNotice({ kind: 'running' })
      } else {
        setNotice({ kind: 'fault', message: describe(thrown) })
      }
    } finally {
      setBusy(false)
    }
  }

  return (
    <Panel
      title="Runtime publication"
      action={
        <button type="button" disabled={busy} onClick={() => void publish()}>
          {busy ? 'Publishing…' : 'Publish now'}
        </button>
      }
    >
      <div className="panel-body">
        {/* `role="status"` for `running` and `ran`: neither is a fault, so neither interrupts a screen reader the way `.error` below does. */}
        {notice?.kind === 'running' && (
          <p role="status">A pass is already running. Refresh status in a moment to see what it did.</p>
        )}
        {notice?.kind === 'ran' && <p role="status">Pass finished — {capitalise(notice.outcome)}{notice.detail !== null && ` — ${notice.detail}`}</p>}
        {notice?.kind === 'fault' && (
          <p role="alert" className="error">
            {notice.message}
          </p>
        )}

        <PublicationRows publication={display} />

        <p className="support-note">
          What the publication target holds and what the last pass did — not whether the runtime has
          reloaded.
        </p>
      </div>
    </Panel>
  )
}
