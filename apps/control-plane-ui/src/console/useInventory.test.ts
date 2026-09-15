import { act, renderHook, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { getIdentity } from '../api/client'
import type { Client, Identity } from '../api/types'
import { useInventory } from './useInventory'

vi.mock('../api/client', () => ({ getIdentity: vi.fn() }))

const clients: Client[] = Array.from({ length: 8 }, (_, index) => ({
  id: `client-${String(index)}`,
  displayName: `Client ${String(index)}`,
  hosts: [],
  realm: `client-${String(index)}`,
  revision: 'r',
}))

const identity: Identity = {
  realm: 'realm',
  roles: [],
  clients: [],
  revision: 'r',
  apiVersion: 'v2',
  reconciliation: { status: 'applied', observedAtUnix: null, detail: null },
}

describe('identity inventory', () => {
  it('limits concurrent reads and exposes each partial failure', async () => {
    const pending: (() => void)[] = []
    vi.mocked(getIdentity).mockImplementation(
      (id) =>
        new Promise((resolve, reject) => {
          pending.push(() => {
            if (id === 'client-1') reject(new Error('Unavailable'))
            else resolve(identity)
          })
        }),
    )

    const { result } = renderHook(() => useInventory(clients))
    expect(pending).toHaveLength(4)

    await act(async () => {
      for (const done of pending.splice(0)) done()
      await Promise.resolve()
    })
    expect(pending).toHaveLength(4)

    await act(async () => {
      for (const done of pending.splice(0)) done()
      await Promise.resolve()
    })

    await waitFor(() => {
      expect(result.current.loading).toBe(false)
    })
    expect(result.current.entries).toHaveLength(8)
    expect(result.current.entries.find((entry) => entry.client.id === 'client-1')?.error).toBe(
      'Unavailable',
    )
  })

  it('ignores results from an old client list', async () => {
    let settle: (value: Identity) => void = () => undefined
    vi.mocked(getIdentity).mockImplementation(
      () =>
        new Promise((resolve) => {
          settle = resolve
        }),
    )

    const { result, rerender } = renderHook(({ list }) => useInventory(list), {
      initialProps: { list: clients.slice(0, 1) },
    })
    rerender({ list: [] })

    await act(async () => {
      settle(identity)
      await Promise.resolve()
    })

    expect(result.current.entries).toEqual([])
    expect(result.current.loading).toBe(false)
  })
})
