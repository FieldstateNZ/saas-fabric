import { useCallback, useEffect, useState } from 'react'

import { getIdentity, putIdentity } from '../api/client'
import { isControlPlaneError } from '../api/errors'
import type { Identity } from '../api/types'
import { describe, type Loadable } from './useClients'

/** Reading and writing one client's identity. */
export interface IdentityEditor extends Loadable<Identity> {
  /** Replaces the roles, keeping everything else as it is. */
  save: (roles: readonly string[]) => Promise<void>
  /** Whether a write is in flight. */
  saving: boolean
}

/**
 * Loads a client's identity, and writes changes back at the revision it was
 * read at.
 *
 * # Conflicts re-read, and keep the explanation
 *
 * When the control plane refuses a write because the client changed, this
 * reloads and reports it. Retrying automatically would be the wrong thing: the
 * operator's edit was made against state that no longer exists, and applying it
 * anyway is the lost update the revision check exists to prevent.
 *
 * The reload carries the conflict message through with it. Without that, the
 * operator would watch their edit revert with nothing on screen saying why —
 * which looks exactly like the console losing their work.
 */
export function useIdentity(clientId: string | null): IdentityEditor {
  const [state, setState] = useState<Loadable<Identity>>({
    value: null,
    loading: clientId !== null,
    error: null,
  })
  const [saving, setSaving] = useState(false)

  const load = useCallback(
    (carried: string | null = null) => {
      if (clientId === null) {
        setState({ value: null, loading: false, error: null })
        return
      }

      setState({ value: null, loading: true, error: carried })

      getIdentity(clientId)
        .then((identity) => {
          setState({ value: identity, loading: false, error: carried })
        })
        .catch((error: unknown) => {
          setState({ value: null, loading: false, error: describe(error) })
        })
    },
    [clientId],
  )

  useEffect(() => {
    load()
  }, [load])

  const save = useCallback(
    async (roles: readonly string[]) => {
      const current = state.value
      if (clientId === null || current === null) {
        return
      }

      setSaving(true)
      try {
        const updated = await putIdentity(clientId, current.revision, {
          realm: current.realm,
          roles,
          clients: current.clients,
        })
        setState({ value: updated, loading: false, error: null })
      } catch (error: unknown) {
        const message = describe(error)

        if (isControlPlaneError(error) && error.isConflict) {
          load(message)
        } else {
          // Everything else keeps the operator's edit on screen: the state they
          // were editing is still current, so their work is still applicable.
          setState({ value: current, loading: false, error: message })
        }
      } finally {
        setSaving(false)
      }
    },
    [clientId, load, state.value],
  )

  return { ...state, save, saving }
}
