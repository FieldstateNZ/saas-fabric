import { useEffect, useState } from 'react'

import { getPlatformIntegration } from '../api/client'
import type { PlatformIntegration } from '../api/types'
import { describe, type Loadable } from './useClients'

/**
 * Loads the Platform Management application's lifecycle.
 *
 * Separate from [`usePlatform`](./usePlatform), and both are needed to say what
 * state this integration is in. This one knows whether an application exists,
 * is installed and has a repository; only the other knows whether reading
 * through it works. Neither can answer alone, and folding them into one route
 * would be two facts with one place to disagree.
 */
export function usePlatformIntegration(): Loadable<PlatformIntegration> {
  const [state, setState] = useState<Loadable<PlatformIntegration>>({
    value: null,
    loading: true,
    error: null,
  })

  useEffect(() => {
    let current = true

    getPlatformIntegration()
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
