import { useEffect, useState } from 'react'

import type { Client } from '../api/types'
import { useIdentity } from '../hooks/useIdentity'
import { ApplicationClients } from './ApplicationClients'
import { ReconciliationBadge } from './ReconciliationBadge'
import { RoleEditor } from './RoleEditor'

/**
 * A client's identity: what it should be, and whether it is.
 *
 * # Only what the domain supports is editable
 *
 * Roles can be changed. The realm cannot -- moving a client to a different
 * realm would abandon every user and session in the old one, and the API
 * refuses it. Applications are shown and not edited in this increment.
 *
 * The console is deliberately not an identity-provider administration tool
 * (section 16): an operator manages SaaS Fabric concepts here, and there is no
 * control anywhere that opens the platform service underneath.
 */
export function IdentityPanel({ client }: { client: Client }) {
  const identity = useIdentity(client.id)
  const [roles, setRoles] = useState<readonly string[]>([])

  useEffect(() => {
    setRoles(identity.value?.roles ?? [])
  }, [identity.value])

  if (identity.loading) {
    return <p className="empty">Loading identity...</p>
  }

  const current = identity.value
  if (current === null) {
    return <p className="error">{identity.error ?? 'This client has no identity configuration.'}</p>
  }

  const changed = roles.join(' ') !== current.roles.join(' ')

  return (
    <section className="identity">
      <h2>Identity</h2>
      {identity.error !== null && <p className="error">{identity.error}</p>}

      <ReconciliationBadge reconciliation={current.reconciliation} />

      <h3>Realm</h3>
      <p className="identity__realm">
        {current.realm}
        <span className="identity__note">
          A client&apos;s realm cannot be changed once it exists.
        </span>
      </p>

      <h3>Realm roles</h3>
      <RoleEditor roles={roles} disabled={identity.saving} onChange={setRoles} />

      <div className="identity__actions">
        <button
          type="button"
          disabled={!changed || identity.saving}
          onClick={() => {
            void identity.save(roles)
          }}
        >
          {identity.saving ? 'Saving...' : 'Save changes'}
        </button>
        <button
          type="button"
          disabled={!changed || identity.saving}
          onClick={() => {
            setRoles(current.roles)
          }}
        >
          Discard
        </button>
      </div>

      <h3>Applications</h3>
      <ApplicationClients clients={current.clients} />
    </section>
  )
}
