import { useCallback, useEffect, useState } from 'react'

import { getIdentity, putIdentity } from '../api/client'
import { isControlPlaneError } from '../api/errors'
import { reassertIdentity } from '../api/reassertion'
import type { Identity } from '../api/types'
import { describe, type Loadable } from './useClients'

/** Reading and writing one client's identity. */
export interface IdentityEditor extends Loadable<Identity> {
  /** Replaces the roles, keeping everything else as it is. */
  save: (roles: readonly string[]) => Promise<void>
  /** Whether a write is in flight. */
  saving: boolean
  /**
   * Overwrites the identity provider's unsupported hand edits with the
   * declared identity, at the revision on screen (D01-19b).
   */
  reassert: () => Promise<void>
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

  /**
   * Runs one write at the revision on screen. Both writes answer with the
   * client's identity, and both are refused the same way on a conflict.
   */
  const write = useCallback(
    async (perform: (clientId: string, current: Identity) => Promise<Identity>) => {
      const current = state.value
      if (clientId === null || current === null) {
        return
      }

      setSaving(true)
      try {
        setState({ value: await perform(clientId, current), loading: false, error: null })
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

  const save = useCallback(
    (roles: readonly string[]) =>
      write((id, current) =>
        putIdentity(id, current.revision, { realm: current.realm, roles, clients: current.clients }),
      ),
    [write],
  )

  const reassert = useCallback(() => write((id, current) => reassertIdentity(id, current.revision)), [write])

  return { ...state, save, saving, reassert }
}
