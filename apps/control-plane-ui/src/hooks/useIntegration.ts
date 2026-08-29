import { useEffect, useState } from 'react'

import { getIntegration } from '../api/client'
import type { Integration } from '../api/types'
import { describe, type Loadable } from './useClients'

/**
 * Loads the state of the platform's connection to client desired state.
 *
 * Asked before anything else the console shows, because it decides *what* to
 * show. A platform nobody has connected yet has no client list to fail to
 * load, and rendering "could not load clients" over that would send an
 * operator looking for a fault that does not exist.
 */
export function useIntegration(): Loadable<Integration> {
  const [state, setState] = useState<Loadable<Integration>>({
    value: null,
    loading: true,
    error: null,
  })

  useEffect(() => {
    let current = true

    getIntegration()
      .then((integration) => {
        if (current) {
          setState({ value: integration, loading: false, error: null })
        }
      })
      .catch((error: unknown) => {
        if (current) {
          setState({ value: null, loading: false, error: describe(error) })
        }
      })

    return () => {
      current = false
    }
  }, [])

  return state
}
