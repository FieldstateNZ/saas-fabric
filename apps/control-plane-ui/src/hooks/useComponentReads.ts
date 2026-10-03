import { useEffect, useState } from 'react'

import { registryReads } from '../api/registries'
import type { ComponentReads } from '../api/registry-types'
import { describe } from './useClients'

/** Which registry reads each managed component's images, or why that is not known. */
export interface ComponentReadsState {
  readonly value: ComponentReads | null
  readonly error: string | null
}

/**
 * `GET /api/integrations/registries/reads`, read again whenever `listing`
 * -- the registries as last read or changed -- is replaced.
 *
 * # Why it follows the listing
 *
 * How an image is read turns on the registry records: a credential set or
 * refused, a repository registered, a registry removed. Each change replaces
 * the listing, and reading this again then keeps the account on each card
 * from describing records that are gone. A read that fails keeps nothing
 * stale: it says it could not be read.
 */
export function useComponentReads(listing: unknown): ComponentReadsState {
  const [state, setState] = useState<ComponentReadsState>({ value: null, error: null })

  useEffect(() => {
    if (listing === null) {
      return undefined
    }
    let current = true
    registryReads().then(
      (value) => {
        if (current) {
          setState({ value, error: null })
        }
      },
      (error: unknown) => {
        if (current) {
          setState({ value: null, error: describe(error) })
        }
      },
    )
    return () => {
      current = false
    }
  }, [listing])

  return state
}
