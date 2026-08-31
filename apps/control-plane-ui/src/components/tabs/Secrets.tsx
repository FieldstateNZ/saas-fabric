import { useState } from 'react'

import type { Client } from '../../api/types'
import { useSecrets } from '../../hooks/useSecrets'
import { SecretRow } from './SecretRow'
import { NewSecret } from './NewSecret'

/**
 * Managing one client's secrets.
 *
 * # Deliberately plain
 *
 * A table, some inputs and some buttons. What matters here is not appearance
 * but which values are on screen: a listing shows paths and never values, and
 * a value appears only after somebody asks for that one secret.
 *
 * # A stale write is not an error to apologise for
 *
 * It means somebody else changed the secret while this operator was looking at
 * it. The console says exactly that and offers to reload, because "500" would
 * send them to an incident channel for a working system behaving correctly.
 */
export function Secrets({ client }: { client: Client }) {
  const secrets = useSecrets(client.id)
  const [notice, setNotice] = useState<string | null>(null)

  if (secrets.state.status === 'loading') {
    return <p className="empty">Loading secrets…</p>
  }

  if (secrets.state.status === 'failed') {
    return (
      <section className="secrets">
        <p className="error">{secrets.state.error}</p>
      </section>
    )
  }

  const { entries } = secrets.state

  return (
    <section className="secrets">
      {notice !== null && <p className="secrets__notice">{notice}</p>}

      {entries.length === 0 ? (
        <p className="empty">This client has no secrets yet.</p>
      ) : (
        <table className="secrets__table">
          <thead>
            <tr>
              <th scope="col">Path</th>
              <th scope="col">Value</th>
              <th scope="col">Actions</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((entry) => (
              <SecretRow
                key={entry.path}
                path={entry.path}
                secrets={secrets}
                onNotice={setNotice}
              />
            ))}
          </tbody>
        </table>
      )}

      <NewSecret secrets={secrets} onNotice={setNotice} />
    </section>
  )
}
