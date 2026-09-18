import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import type { ApplicationDefinition, ProductApplication } from '../api/catalogue-types'
import { ApplicationWorkspace } from './ApplicationWorkspace'
import type { CatalogueState } from './useCatalogue'

const definition: ApplicationDefinition = {
  name: 'Portal',
  description: '',
  domain: '',
  components: [],
  features: [],
  plans: [],
  fields: [],
  navigation: [],
  resources: [],
}

function state(overrides: Partial<CatalogueState> = {}): CatalogueState {
  return {
    value: null,
    loadError: null,
    loading: false,
    saving: false,
    saveError: null,
    conflict: false,
    navigatedAwayNotice: null,
    dismissNavigatedAwayNotice: vi.fn(),
    refresh: vi.fn(),
    clearSaveError: vi.fn(),
    setCurrentPage: vi.fn(),
    save: vi.fn().mockResolvedValue(true),
    ...overrides,
  }
}

describe('ApplicationWorkspace: Reload is offered only for a conflict', () => {
  const draftApp: ProductApplication = { id: 'portal', draft: definition, releases: [] }

  it('shows no Reload button for a 400 validation failure', () => {
    render(<ApplicationWorkspace app={draftApp} state={state({ saveError: 'name is required' })} />)

    expect(screen.getByText('name is required')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Reload latest version/ })).not.toBeInTheDocument()
  })

  it('offers Reload for a 409 conflict', () => {
    render(<ApplicationWorkspace app={draftApp} state={state({ saveError: 'stale', conflict: true })} />)

    expect(screen.getByRole('button', { name: /Reload latest version/ })).toBeInTheDocument()
  })
})

describe('ApplicationWorkspace: published compares the live draft, not the last-saved one', () => {
  it('shows Draft changes the moment a local edit diverges, before it is saved', async () => {
    const published: ProductApplication = {
      id: 'portal',
      draft: definition,
      releases: [{ version: 1, note: 'first', publishedAt: 1700000000, definition }],
    }
    const user = userEvent.setup()

    render(<ApplicationWorkspace app={published} state={state()} />)
    expect(screen.getByText('Published')).toBeInTheDocument()

    await user.type(screen.getByRole('textbox', { name: 'Name *' }), ' X')

    expect(screen.getByText('Draft changes')).toBeInTheDocument()
    expect(screen.queryByText('Published')).not.toBeInTheDocument()
  })
})
