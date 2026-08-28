import { useEffect, useState } from 'react'

import { listClients } from '../api/client'
import type { Client } from '../api/types'

/** What a load can be doing. */
export interface Loadable<T> {
  readonly value: T | null
  readonly loading: boolean
  readonly error: string | null
}

/**
 * Loads the client list.
 *
 * Once, on mount, with no refresh control — deliberately. Nothing this
 * increment can do changes what the list shows: a client's display name and
 * domains are not editable here, and the revision the list carries is not
 * displayed. A refresh button would be a control that never has anything to do.
 *
 * It grows one when the console can create a client.
 */
export function useClients(): Loadable<readonly Client[]> {
  const [state, setState] = useState<Loadable<readonly Client[]>>({
    value: null,
    loading: true,
    error: null,
  })

  useEffect(() => {
    let current = true

    listClients()
      .then((clients) => {
        if (current) {
          setState({ value: clients, loading: false, error: null })
        }
      })
      .catch((error: unknown) => {
        if (current) {
          setState({ value: null, loading: false, error: describe(error) })
        }
      })

    // React's strict mode mounts effects twice in development. Without this,
    // the second load's result can land before the first's and the console
    // renders a list it has already replaced.
    return () => {
      current = false
    }
  }, [])

  return state
}

/** Turns anything thrown into something an operator can read. */
export function describe(error: unknown): string {
  return error instanceof Error ? error.message : 'Something went wrong.'
}
