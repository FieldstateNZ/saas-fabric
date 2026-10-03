import { useEffect, useState } from 'react'

import { listRegistries, registryVersions, underHost } from '../../api/registries'
import type { RegistryListing, RegistryVersions } from '../../api/registry-types'
import { describe } from '../../hooks/useClients'

/** One read the picker waits on: its answer, or why there is none. */
export interface Read<T> {
  readonly value: T | null
  readonly loading: boolean
  readonly error: string | null
}

/**
 * The registries an operator registered, read once when the picker opens.
 *
 * Read here rather than through `useRegistries`, which carries every change
 * an operator can make to a registry: the picker changes none, and only
 * navigates what is registered (ADR 0026 section 7).
 */
export function useRegistryListing(): Read<RegistryListing> {
  const [read, setRead] = useState<Read<RegistryListing>>({
    value: null,
    loading: true,
    error: null,
  })

  useEffect(() => {
    let current = true
    listRegistries().then(
      (value) => {
        if (current) {
          setRead({ value, loading: false, error: null })
        }
      },
      (error: unknown) => {
        if (current) {
          setRead({ value: null, loading: false, error: describe(error) })
        }
      },
    )
    return () => {
      current = false
    }
  }, [])

  return read
}

/**
 * `repository`'s version tags under `host`, newest first, read again
 * whenever either changes. A read overtaken by another choice is dropped,
 * so an older answer never replaces a newer one.
 *
 * The list is navigation, not validation: a tag listed here is only one
 * that parses as a version, and selecting it is what the server checks.
 */
export function useVersions(host: string, repository: string): Read<RegistryVersions> {
  const [read, setRead] = useState<Read<RegistryVersions>>({
    value: null,
    loading: false,
    error: null,
  })

  useEffect(() => {
    if (host === '' || repository === '') {
      setRead({ value: null, loading: false, error: null })
      return undefined
    }
    const under = underHost(host, repository)
    if ('problem' in under) {
      setRead({ value: null, loading: false, error: under.problem })
      return undefined
    }
    let current = true
    setRead({ value: null, loading: true, error: null })
    registryVersions(host, under.path).then(
      (value) => {
        if (current) {
          setRead({ value, loading: false, error: null })
        }
      },
      (error: unknown) => {
        if (current) {
          setRead({ value: null, loading: false, error: describe(error) })
        }
      },
    )
    return () => {
      current = false
    }
  }, [host, repository])

  return read
}
