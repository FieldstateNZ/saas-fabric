import { useState } from 'react'

import type { IntegrationEndpoints } from '../api/client'
import type { Application, ConnectionState } from '../api/types'
import { describe } from '../hooks/useClients'
import { ConnectApplication } from './ConnectApplication'
import { RepositoryPicker } from './RepositoryPicker'

/**
 * One integration, as an operator reads and acts on it.
 *
 * # Three states, three different actions
 *
 * `unavailable` is the one worth having. An integration that exists and does
 * not work must not read as "not connected", because an operator shown that
 * goes and connects it a second time instead of finding out why the first one
 * stopped. So this card never offers **Connect** in that state — only
 * **Reconnect**, which is a deliberate click because it creates a second
 * application, and **Disconnect**.
 *
 * Everywhere else the outstanding step is shown inline rather than behind a
 * button that reveals a button: an application waiting to be installed shows
 * "Install on GitHub", and an installation reaching several repositories shows
 * the list.
 *
 * # What is deliberately not here
 *
 * Any environment policy. Connecting a repository establishes *authority*;
 * whether an environment advances automatically is a different concern with a
 * different blast radius, and it belongs beside the environment it governs.
 */
interface IntegrationCardProps {
  /** What this integration is called, in the operator's words. */
  readonly name: string

  /** What connecting it lets the platform do. */
  readonly purpose: string

  readonly state: ConnectionState
  readonly endpoints: IntegrationEndpoints
  readonly application: Application | null

  /** Why it is unavailable, when it is. Operator-safe text, from the API. */
  readonly diagnostic: string | null

  /** What this deployment does instead, when it connects nothing. */
  readonly unmanaged: string
}

/** What each state is called on screen. */
const WORDS: Readonly<Record<ConnectionState, string>> = {
  connected: 'Connected',
  unavailable: 'Unavailable',
  'not-connected': 'Not connected',
  'not-managed': 'Not managed',
}

export function IntegrationCard(props: IntegrationCardProps) {
  const { name, purpose, state, endpoints, application, diagnostic, unmanaged } = props
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [reconnecting, setReconnecting] = useState(false)

  async function disconnect(): Promise<void> {
    setBusy(true)
    setError(null)

    try {
      await endpoints.disconnect()
      window.location.reload()
    } catch (thrown: unknown) {
      setError(describe(thrown))
      setBusy(false)
    }
  }

  if (state === 'not-managed') {
    return (
      <section className="integration">
        <h3 className="integration__name">{name}</h3>
        <p className="integration__state integration__state--not-managed">
          {WORDS['not-managed']}
        </p>
        <p className="integration__detail">{unmanaged}</p>
      </section>
    )
  }

  // Installed, and nobody has said which repository. The platform declines to
  // guess between them; this is where the operator answers.
  const undecided = application?.installed === true && application.repository === null

  // The step that is actually outstanding, shown without being asked for.
  const outstanding = state === 'not-connected' && !undecided

  return (
    <section className="integration">
      <h3 className="integration__name">{name}</h3>
      <p className={`integration__state integration__state--${state}`}>{WORDS[state]}</p>

      <dl className="integration__rows">
        <dt>Repository</dt>
        <dd>{application?.repository ?? 'None'}</dd>
      </dl>

      {diagnostic !== null && <p className="integration__diagnostic">{diagnostic}</p>}
      {error !== null && <p className="error">{error}</p>}

      {undecided && <RepositoryPicker endpoints={endpoints} />}

      {(outstanding || reconnecting) && (
        <ConnectApplication
          endpoints={endpoints}
          // A reconnect starts over: the key arrived once and cannot be asked
          // for again, so what fixes a broken application is a new one.
          application={reconnecting ? null : application}
          purpose={purpose}
        />
      )}

      <div className="integration__actions">
        {state === 'unavailable' && !reconnecting && (
          <button
            type="button"
            className="integration__action"
            disabled={busy}
            onClick={() => {
              setReconnecting(true)
            }}
          >
            Reconnect
          </button>
        )}

        {application !== null && (
          <button
            type="button"
            className="integration__action integration__action--quiet"
            disabled={busy}
            onClick={() => void disconnect()}
          >
            {busy ? 'Disconnecting…' : 'Disconnect'}
          </button>
        )}
      </div>
    </section>
  )
}
