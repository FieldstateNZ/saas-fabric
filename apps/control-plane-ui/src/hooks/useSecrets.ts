/**
 * One client's secrets, and the operations on them.
 *
 * # Revealed values live here and nowhere else
 *
 * They are held in component state for as long as the operator is looking at
 * them, and never written to `localStorage` or `sessionStorage`. The response
 * that carries them says `no-store`; keeping a copy somewhere the browser
 * persists would defeat that at the last step.
 */
import { useCallback, useEffect, useState } from 'react'

import { deleteSecret, listSecrets, revealSecret, writeSecret } from '../api/client'
import type { SecretEntry } from '../api/types'
import { describe } from './useClients'

/** Where the listing stands. */
export type SecretsState =
  | { status: 'loading' }
  | { status: 'ready'; entries: readonly SecretEntry[] }
  | { status: 'failed'; error: string }

/** The listing, and the things an operator can do to it. */
export interface Secrets {
  readonly state: SecretsState
  readonly reload: () => void
  readonly reveal: (path: string) => Promise<Readonly<Record<string, string>>>
  readonly write: (
    path: string,
    values: Record<string, string>,
    expectedVersion: number | null,
  ) => Promise<void>
  readonly remove: (path: string) => Promise<void>
}

export function useSecrets(client: string): Secrets {
  const [state, setState] = useState<SecretsState>({ status: 'loading' })
  const [reloads, setReloads] = useState(0)

  useEffect(() => {
    let abandoned = false

    setState({ status: 'loading' })

    listSecrets(client).then(
      (entries) => {
        if (!abandoned) {
          setState({ status: 'ready', entries })
        }
      },
      (error: unknown) => {
        if (!abandoned) {
          setState({ status: 'failed', error: describe(error) })
        }
      },
    )

    return () => {
      abandoned = true
    }
  }, [client, reloads])

  const reload = useCallback(() => {
    setReloads((count) => count + 1)
  }, [])

  const reveal = useCallback(
    async (path: string) => (await revealSecret(client, path)).values,
    [client],
  )

  const write = useCallback(
    async (path: string, values: Record<string, string>, expectedVersion: number | null) => {
      await writeSecret(client, path, values, expectedVersion)
      setReloads((count) => count + 1)
    },
    [client],
  )

  const remove = useCallback(
    async (path: string) => {
      await deleteSecret(client, path)
      setReloads((count) => count + 1)
    },
    [client],
  )

  return { state, reload, reveal, write, remove }
}
