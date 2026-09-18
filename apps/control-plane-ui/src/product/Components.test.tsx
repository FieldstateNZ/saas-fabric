import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Catalogue } from '../api/catalogue-types'
import type { PlatformState } from '../hooks/usePlatform'
import { Components } from './Components'

const platform: PlatformState = { value: null, loading: false, error: null, unmanaged: true }

const catalogue: Catalogue = {
  applications: [
    {
      id: 'portal',
      draft: {
        name: 'Portal',
        description: '',
        domain: '',
        components: [
          {
            id: 'web',
            name: 'Web',
            kind: 'container',
            reference: 'ghcr.io/x/web',
            version: '',
            required: true,
            policy: 'automatic',
          },
          {
            id: 'idp',
            name: 'Identity',
            kind: 'capability',
            reference: 'Identity',
            version: '',
            required: true,
            policy: 'manual',
          },
        ],
        features: [],
        plans: [],
        fields: [],
        navigation: [],
        resources: [],
      },
      releases: [],
    },
  ],
  clientFields: [],
  settings: { platformName: 'Fieldstate', defaultRegion: 'New Zealand', timezone: 'Pacific/Auckland' },
  environments: [],
  activity: [],
  definitionVersion: 1,
}

describe('Components: an unpinned component says so, and only a capability is platform-provided', () => {
  it('shows "Not pinned" for an empty version, and "Platform capability" only for a capability', () => {
    render(<Components catalogue={catalogue} platform={platform} />)

    expect(screen.getByText('Not pinned')).toBeInTheDocument()
    expect(screen.getByText('Platform capability')).toBeInTheDocument()
  })
})
