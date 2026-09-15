import { useCallback, useEffect, useState } from 'react'
import { getIdentity } from '../api/client'
import type { Client, Identity } from '../api/types'
import { describe } from '../hooks/useClients'

export interface IdentityEntry { client: Client; identity: Identity | null; error: string | null }
export interface Inventory { entries: readonly IdentityEntry[]; loading: boolean; refresh: () => void }

/** A bounded number of requests, with partial failures kept visible. No guessed health. */
export function useInventory(clients: readonly Client[] | null): Inventory {
  const [entries, setEntries] = useState<readonly IdentityEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [generation, setGeneration] = useState(0)
  const refresh = useCallback(() => { setGeneration((value) => value + 1) }, [])
  useEffect(() => {
    let active = true
    if (clients === null) return
    setLoading(true)
    async function load() {
      const results: IdentityEntry[] = []
      let next = 0
      async function worker() {
        while (active && next < (clients?.length ?? 0)) {
          const client = clients?.[next++]
          if (!client) return
          try {
            results.push({ client, identity: await getIdentity(client.id), error: null })
          } catch (error: unknown) {
            results.push({ client, identity: null, error: describe(error) })
          }
        }
      }
      await Promise.all(Array.from({ length: Math.min(4, clients?.length ?? 0) }, worker))
      if (active) {
        results.sort((a, b) => a.client.displayName.localeCompare(b.client.displayName))
        setEntries(results)
        setLoading(false)
      }
    }
    void load()
    return () => { active = false }
  }, [clients, generation])
  return { entries, loading, refresh }
}
