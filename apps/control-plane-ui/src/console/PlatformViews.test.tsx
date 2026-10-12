/**
 * The Environments page renders the runtime-publication row.
 *
 * A narrow placement test, not a re-test of `PublicationPanel` itself
 * (`PublicationPanel.test.tsx` already covers its own behaviour): this pins
 * that `PlatformViews` actually mounts it beneath the environment card, from
 * exactly the JSON shape `GET /api/platform` sends.
 */
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Platform } from '../api/types'
import type { PlatformState } from '../hooks/usePlatform'
import { PlatformViews } from './PlatformViews'

function platformState(): PlatformState {
  const value: Platform = {
    environment: 'lucentroot',
    components: [],
    lastCheck: null,
    publication: {
      target: 'ConfigMaps in namespace platform-system',
      documents: { tenants: 3, dataSources: 2, catalog: 1, registry: null },
      lastPass: { atUnixSeconds: 1_700_000_000, outcome: 'published', detail: null },
      completeness: {
        state: 'complete',
        observedAtUnixSeconds: 1_700_000_000,
        record: { generation: 1, recordSha256: 'ab'.repeat(32) },
        reason: null,
      },
      readiness: {
        documents: { met: true, reasons: [] },
        identity: { met: false, reasons: ['Fabric publishes no issuer registry'] },
        connectors: { met: false, reasons: ['platform composition must deploy and configure postgres-nz'] },
        excludedFromRegistry: [],
        registryApplication: { state: 'unobserved', detail: 'no runtime reads this registry' },
        readers: {
          state: 'unobserved',
          reasons: ['no reader observation is configured'],
          expected: null,
          observedAtUnixSeconds: null,
          desiredReplicas: null,
          members: [],
        },
        updateComplete: false,
      },
    },
  }

  return { value, loading: false, error: null, unmanaged: false }
}

/**
 * The `<dd>` beside the `<dt>` named `label` -- bound this way rather than
 * `getByText('revision 3')` alone, so a `tenants`/`dataSources` mix-up in
 * `PublicationRows` would fail this test instead of passing it by accident.
 */
function fact(label: string): Element | null {
  return screen.getByText(label).nextElementSibling
}

describe('the Environments page', () => {
  it('shows the publication target and its held revisions beneath the environment card', () => {
    render(<PlatformViews platform={platformState()} environments />)

    expect(screen.getByText('ConfigMaps in namespace platform-system')).toBeInTheDocument()
    expect(fact('Tenants')).toHaveTextContent(/^revision 3$/)
  })
})
