import { useCallback, useEffect, useState } from 'react'

import { getIdentity } from '../api/client'
import type { Client, Identity } from '../api/types'
import { describe } from '../hooks/useClients'

/** One client's identity read, or why it could not be read. */
export interface IdentityEntry {
  readonly client: Client
  readonly identity: Identity | null
  readonly error: string | null
}

/** Every client's identity, read for the dashboard, directory, and reconciliation pages to share. */
export interface Inventory {
  readonly entries: readonly IdentityEntry[]
  readonly loading: boolean
  readonly refresh: () => void
}

/**
 * Reads every client's identity, four requests at a time.
 *
 * # Why bounded, and why partial failure stays visible
 *
 * One request per client, unbounded, is a client list's worth of connections
 * fired at once — fine for a handful of clients, not for a platform's whole
 * directory. Four workers pull from a shared cursor instead, so the total
 * stays bounded regardless of how many clients there are.
 *
 * A client whose identity fails to load keeps its row, carrying the error
 * instead of the identity — see {@link IdentityEntry}. Dropping it would
 * quietly under-report a metric or an inventory row; a console that shows
 * only what it observed does not get to skip the requests it lost.
 *
 * # No guessed health
 *
 * There is no polling here and no assumption that a client not yet read is
 * healthy — `loading` covers the whole read, and callers that need to tell
 * "still loading" from "known and fine" (see `Dashboard`) must check it
 * rather than infer it from an empty entry.
 */
export function useInventory(clients: readonly Client[] | null): Inventory {
  const [entries, setEntries] = useState<readonly IdentityEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [generation, setGeneration] = useState(0)

  const refresh = useCallback(() => {
    setGeneration((value) => value + 1)
  }, [])

  useEffect(() => {
    let active = true
    if (clients === null) {
      return
    }

    setLoading(true)

    async function load() {
      const results: IdentityEntry[] = []
      let next = 0

      async function worker() {
        while (active && next < (clients?.length ?? 0)) {
          const client = clients?.[next++]
          if (!client) {
            return
          }

          try {
            results.push({ client, identity: await getIdentity(client.id), error: null })
          } catch (error: unknown) {
            results.push({ client, identity: null, error: describe(error) })
          }
        }
      }

      await Promise.all(Array.from({ length: Math.min(4, clients?.length ?? 0) }, worker))

      // A generation that has since moved on, or a component that has since
      // unmounted, leaves nothing worth committing: a later request may
      // already be filling `entries` with its own answer.
      if (active) {
        results.sort((a, b) => a.client.displayName.localeCompare(b.client.displayName))
        setEntries(results)
        setLoading(false)
      }
    }

    void load()
    return () => {
      active = false
    }
  }, [clients, generation])

  return { entries, loading, refresh }
}
