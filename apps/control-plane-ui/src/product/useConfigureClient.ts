import { useState } from 'react'

import { getProduct } from '../api/catalogue'
import type { ClientProductResponse } from '../api/catalogue-types'
import { describe } from '../hooks/useClients'

/**
 * The state behind `ClientWorkspace`'s Configure: a fresh read before the
 * form opens, and the notice that appears when the form closes because the
 * client changed instead of because it saved.
 *
 * Pulled out of `ClientWorkspace` on its own — the read, the in-flight flag
 * that disables the rest of the workspace while it runs, and the stale
 * notice are one small piece of behaviour that does not need the render
 * tree around it to be understood or tested.
 */
export interface ConfigureClient {
  /** Whether the fresh read Configure starts with is in flight. */
  readonly opening: boolean
  /** Whether the form last closed because the client changed underneath it, not because it saved. */
  readonly staleNotice: boolean
  /** Starts the fresh read, then hands the result to `onOpened` or the failure to `onFailed`. */
  readonly open: () => Promise<void>
  /** Call when `ClientForm`'s `onStale` fires, so the notice appears once the form closes. */
  readonly noteStale: () => void
  /** Clears the notice once the operator has moved on to something else. */
  readonly dismissStaleNotice: () => void
}

export function useConfigureClient(
  id: string,
  handlers: {
    readonly onOpened: (fresh: ClientProductResponse) => void
    readonly onFailed: (message: string) => void
  },
): ConfigureClient {
  const [opening, setOpening] = useState(false)
  const [staleNotice, setStaleNotice] = useState(false)

  async function open() {
    if (opening) {
      return
    }

    setOpening(true)
    setStaleNotice(false)
    try {
      handlers.onOpened(await getProduct(id))
    } catch (thrown: unknown) {
      handlers.onFailed(describe(thrown))
    } finally {
      setOpening(false)
    }
  }

  return {
    opening,
    staleNotice,
    open,
    noteStale: () => {
      setStaleNotice(true)
    },
    dismissStaleNotice: () => {
      setStaleNotice(false)
    },
  }
}
