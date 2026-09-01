import { useEffect, useState } from 'react'

import { getPlatform } from '../api/platform'
import { ControlPlaneError } from '../api/errors'
import type { Platform } from '../api/types'
import { describe, type Loadable } from './useClients'

/** A platform this deployment does not manage, told apart from a failure. */
export interface PlatformState extends Loadable<Platform> {
  /**
   * Whether the platform routes exist and this deployment manages nothing.
   *
   * Distinguished from an error because it is not one: it is a state an
   * operator can act on, and rendering "could not load" over it would send
   * them looking for a fault instead of a connection they have not made.
   */
  readonly unmanaged: boolean
}

/** Loads what this deployment's environment is asked to run. */
export function usePlatform(): PlatformState {
  const [state, setState] = useState<PlatformState>({
    value: null,
    loading: true,
    error: null,
    unmanaged: false,
  })

  useEffect(() => {
    let current = true

    getPlatform()
      .then((platform) => {
        if (current) {
          setState({ value: platform, loading: false, error: null, unmanaged: false })
        }
      })
      .catch((error: unknown) => {
        if (!current) {
          return
        }

        const unmanaged =
          error instanceof ControlPlaneError && error.code === 'platform_not_managed'

        setState({
          value: null,
          loading: false,
          error: unmanaged ? null : describe(error),
          unmanaged,
        })
      })

    return () => {
      current = false
    }
  }, [])

  return state
}
