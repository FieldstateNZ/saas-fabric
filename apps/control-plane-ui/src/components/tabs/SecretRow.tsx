import { useState } from 'react'

import type { Secrets } from '../../hooks/useSecrets'
import { describe } from '../../hooks/useClients'

/**
 * One secret: its path, and what an operator can do to it.
 *
 * # A value is on screen only while somebody is looking at it
 *
 * Revealing fetches; hiding drops the values from state. They are never put in
 * `localStorage`, `sessionStorage` or a URL, so closing the tab is enough to
 * be rid of them.
 */
export function SecretRow({
  path,
  secrets,
  onNotice,
}: {
  path: string
  secrets: Secrets
  onNotice: (notice: string | null) => void
}) {
  const [values, setValues] = useState<Record<string, string> | null>(null)
  const [busy, setBusy] = useState(false)

  async function act(what: string, action: () => Promise<void>): Promise<void> {
    setBusy(true)
    onNotice(null)

    try {
      await action()
    } catch (thrown: unknown) {
      onNotice(`${what} ${path}: ${describe(thrown)}`)
    } finally {
      setBusy(false)
    }
  }

  return (
    <tr>
      <td className="secrets__path">{path}</td>

      <td>
        {values === null ? (
          <span className="secrets__hidden">hidden</span>
        ) : (
          <dl className="secrets__values">
            {Object.entries(values).map(([key, value]) => (
              <div key={key}>
                <dt>{key}</dt>
                <dd>
                  <code>{value}</code>
                </dd>
              </div>
            ))}
          </dl>
        )}
      </td>

      <td className="secrets__actions">
        {values === null ? (
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              void act('Could not reveal', async () => {
                setValues(await secrets.reveal(path));
              })
            }}
          >
            Reveal
          </button>
        ) : (
          <button
            type="button"
            onClick={() => {
              // Dropping them from state is the whole of hiding. There is
              // nowhere else a copy could be.
              setValues(null)
            }}
          >
            Hide
          </button>
        )}

        <button
          type="button"
          disabled={busy}
          onClick={() => {
            void act('Could not delete', () => secrets.remove(path))
          }}
        >
          Delete
        </button>
      </td>
    </tr>
  )
}
