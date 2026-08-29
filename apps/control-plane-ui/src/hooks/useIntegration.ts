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

    // The Git host returns the browser here with an outcome in the query
    // string. It is read once and removed, so a reload does not re-announce a
    // connection that happened minutes ago.
    clearCallbackQuery()

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

/** Removes the Git host's outcome from the address bar. */
function clearCallbackQuery(): void {
  const query = new URLSearchParams(window.location.search)

  if (!query.has('git') && !query.has('git_error')) {
    return
  }

  window.history.replaceState({}, '', window.location.pathname)
}
