import { act, renderHook, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import type { Platform } from '../api/types'
import { getPlatform } from '../api/platform'
import { usePlatform } from './usePlatform'

vi.mock('../api/platform', () => ({ getPlatform: vi.fn() }))
afterEach(() => { vi.resetAllMocks() })

it('clears earlier evidence while refreshing and after an unsuccessful read', async () => {
  const result: Platform = { environment: 'lucentroot', components: [], lastCheck: null, publication: null }
  vi.mocked(getPlatform).mockResolvedValueOnce(result)
  const { result: hook } = renderHook(usePlatform)
  await waitFor(() => { expect(hook.current.value).toEqual(result) })
  let rejectRead: ((reason: Error) => void) | undefined
  vi.mocked(getPlatform).mockImplementationOnce(() => new Promise((_resolve, reject) => { rejectRead = reject }))
  act(() => { hook.current.refresh?.() })
  expect(hook.current.loading).toBe(true)
  expect(hook.current.value).toBeNull()
  await act(async () => { rejectRead?.(new Error('Cluster unavailable')); await Promise.resolve() })
  expect(hook.current.loading).toBe(false)
  expect(hook.current.value).toBeNull()
  expect(hook.current.error).toBeTruthy()
  expect(getPlatform).toHaveBeenCalledTimes(2)
})
