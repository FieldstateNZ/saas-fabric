import { useEffect, useState } from 'react'

import { getActivity } from '../api/catalogue'
import type { ProductActivity } from '../api/catalogue-types'
import { describe } from '../hooks/useClients'
import { ActivityTable } from './ActivityTable'

/**
 * The platform-wide activity feed, shown on the Reconciliation page.
 *
 * `generation` is a caller-supplied refresh key (unused by the console
 * today, but load-bearing for a future caller that wants to force a reload
 * without duplicating this component's own refresh button). The button
 * beside it is this component's own: activity is not observed continuously,
 * only read on demand.
 */
export function ProductActivityFeed({ generation = 0 }: { generation?: number }) {
  const [refresh, setRefresh] = useState(0)
  const [activity, setActivity] = useState<ProductActivity[]>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    let active = true

    void getActivity().then(
      (value) => {
        if (active) {
          setActivity(value.activity)
          setError(null)
        }
      },
      (error: unknown) => {
        if (active) {
          setError(describe(error))
        }
      },
    )

    return () => {
      active = false
    }
  }, [generation, refresh])

  return (
    <>
      <button
        type="button"
        onClick={() => {
          setRefresh((n) => n + 1)
        }}
      >
        Refresh activity
      </button>
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <ActivityTable activity={activity} />
    </>
  )
}
