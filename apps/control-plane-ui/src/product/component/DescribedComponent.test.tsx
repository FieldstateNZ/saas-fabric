/**
 * Selecting a component artifact, end to end through the application
 * workspace and a stubbed control plane (ADR 0026 sections 7 and 8).
 *
 * The distinctions under test: the picker offers only what is registered
 * and sends a repository and a version tag, never a digest; it is disabled,
 * and says why, while the draft has other unsaved changes; its answer
 * replaces the local draft; a refusal is worded from the server's answer
 * and reason; a save strips what the server resolved; and declared content
 * is shown read-only, labelled with its component, whatever tab reads it.
 */
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type {
  ApplicationDefinition,
  ApplicationRelease,
  StoredCatalogue,
} from '../../api/catalogue-types'
import type { Registry, RegistryListing } from '../../api/registry-types'
import { ApplicationWorkspace } from '../ApplicationWorkspace'
import { APPLICATION_TABS } from '../applicationWorkspaceTabs'
import { useCatalogue } from '../useCatalogue'
import {
  authored,
  definition,
  described,
  FIRST,
  PRIMARY,
  REPOSITORY,
  resolution,
  SECOND,
} from './described.fixture'

const REGISTRY: Registry = {
  host: 'registry.example.com',
  kind: 'distribution',
  endpoint: 'https://registry.example.com',
  realmOrigin: null,
  deployment: false,
  installed: true,
  credential: null,
  registeredBy: 'brett',
  registeredAt: 1_790_000_000,
  repositories: [{ repository: REPOSITORY, provenAt: 1_790_000_000 }],
}

const VERSIONS_URL = '/api/integrations/registries/registry.example.com/versions/acme/reports'

function listing(...registries: Registry[]): RegistryListing {
  return { registries, deployment: null }
}

function stored(draft: ApplicationDefinition, releases: ApplicationRelease[] = []): StoredCatalogue {
  return {
    catalogue: {
      applications: [{ id: 'analytics', draft, releases }],
      clientFields: [],
      settings: { platformName: 'Fabric', defaultRegion: 'New Zealand', timezone: 'Pacific/Auckland' },
      environments: [],
      activity: [],
      definitionVersion: 1,
    },
    revision: 'rev-1',
  }
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

interface Call {
  readonly method: string
  readonly url: string
  readonly ifMatch: string | null
  readonly body: unknown
}

interface Server {
  catalogue: StoredCatalogue
  listing: RegistryListing
  versions: () => Response
  change: (call: Call) => Response
}

function server(overrides: Partial<Server> = {}): Server {
  return {
    catalogue: stored(definition([])),
    listing: listing(REGISTRY),
    versions: () => json(200, { tags: ['1.5.0', '1.4.0'], other: 2 }),
    change: () => json(500, { error: { code: 'unexpected', message: 'not in this test' } }),
    ...overrides,
  }
}

/** A control plane answering from `state`, recording every call. */
function serve(state: Server): Call[] {
  const calls: Call[] = []
  vi.stubGlobal(
    'fetch',
    vi.fn((url: string, init?: RequestInit) => {
      const call: Call = {
        method: init?.method ?? 'GET',
        url,
        ifMatch: new Headers(init?.headers).get('If-Match'),
        body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
      }
      calls.push(call)
      if (url === '/api/catalogue') {
        return Promise.resolve(call.method === 'GET' ? json(200, state.catalogue) : state.change(call))
      }
      if (url === '/api/integrations/registries') {
        return Promise.resolve(json(200, state.listing))
      }
      if (url === VERSIONS_URL) {
        return Promise.resolve(state.versions())
      }
      return Promise.resolve(json(404, { error: { code: 'unknown', message: url } }))
    }),
  )
  return calls
}

function Harness() {
  const state = useCatalogue()
  const app = state.value?.catalogue.applications[0]
  return app ? <ApplicationWorkspace app={app} state={state} /> : <p>Loading</p>
}

async function openTab(name: string) {
  const tabs = await screen.findByRole('navigation', { name: 'Application sections' })
  await userEvent.click(within(tabs).getByRole('button', { name }))
}

/** Chooses the registry, the repository and `version` in the open picker. */
async function choose(version: string) {
  const picker = await screen.findByRole('region', { name: 'Choose a component artifact' })
  const registry = within(picker).queryByRole('combobox', { name: 'Registry' })
  if (registry && (registry as HTMLSelectElement).value === '') {
    await userEvent.selectOptions(registry, 'registry.example.com')
  }
  const repository = within(picker).getByRole('combobox', { name: 'Repository' })
  if ((repository as HTMLSelectElement).value === '') {
    await userEvent.selectOptions(repository, REPOSITORY)
  }
  await userEvent.selectOptions(await within(picker).findByRole('combobox', { name: 'Version' }), version)
  return picker
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the component picker', () => {
  it('makes a new component described, and lists version tags newest first with how many were not versions', async () => {
    serve(server())
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))

    expect(screen.getByRole('combobox', { name: 'Kind' })).toHaveValue('described')
    const picker = await choose('1.4.0')
    const options = within(within(picker).getByRole('combobox', { name: 'Version' })).getAllByRole('option')

    expect(options.map((option) => option.textContent)).toEqual(['Choose a version…', '1.5.0', '1.4.0'])
    expect(within(picker).getByText('2 tags are not versions and are not listed.')).toBeInTheDocument()
  })

  it('sends the repository and the version tag and no digest, and its answer replaces the local draft', async () => {
    const calls = serve(
      server({
        change: () => json(200, stored(definition([described('reports', resolution('1.4.0', FIRST))]))),
      }),
    )
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Component ID *' }), 'reports')
    const picker = await choose('1.4.0')
    await userEvent.click(within(picker).getByRole('button', { name: 'Use this version' }))

    const post = calls.find((call) => call.method === 'POST')
    expect(post?.body).toEqual({
      action: 'selectComponentVersion',
      id: 'analytics',
      component: 'reports',
      repository: REPOSITORY,
      version: '1.4.0',
    })
    expect(JSON.stringify(post?.body)).not.toContain('sha256')
    expect(post?.ifMatch).toBe('"rev-1"')

    const declared = await screen.findByRole('region', { name: 'Declared by the component descriptor' })
    expect(within(declared).getByText('c'.repeat(40))).toBeInTheDocument()
    expect(within(declared).getAllByTitle(PRIMARY)[0]).toHaveTextContent('sha256:111111111111…')
    expect(within(declared).getByTitle(FIRST)).toHaveTextContent('sha256:333333333333…')
    expect(within(declared).getByText(/\(primary\)/)).toBeInTheDocument()
    expect(within(declared).getByText('No default')).toBeInTheDocument()
    expect(within(declared).getByText('read, list')).toBeInTheDocument()
    expect(within(declared).getByText(/not a claim that the artifact is still there/)).toBeInTheDocument()
    expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument()
  })

  it('is disabled while the draft has other unsaved changes, and says why', async () => {
    const calls = serve(
      server({ catalogue: stored(definition([described('reports', resolution('1.4.0', FIRST))])) }),
    )
    render(<Harness />)
    await openTab('Components')
    const name = screen.getByRole('textbox', { name: 'Component name *' })
    await userEvent.type(name, ' renamed')
    await userEvent.click(screen.getByRole('button', { name: 'Choose another version' }))
    const picker = await choose('1.5.0')

    expect(within(picker).getByRole('combobox', { name: 'Repository' })).toHaveValue(REPOSITORY)
    expect(within(picker).getByRole('button', { name: 'Use this version' })).toBeDisabled()
    expect(within(picker).getByText(/Save or discard the draft’s other changes first/)).toBeInTheDocument()
    expect(calls.some((call) => call.method === 'POST')).toBe(false)
  })

  it('is disabled until a new component has an ID, and a pending component alone does not block it', async () => {
    serve(server())
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    const picker = await choose('1.4.0')

    expect(within(picker).getByRole('button', { name: 'Use this version' })).toBeDisabled()
    expect(within(picker).getByText('Give the component an ID first.')).toBeInTheDocument()

    await userEvent.type(screen.getByRole('textbox', { name: 'Component ID *' }), 'reports')
    expect(within(picker).getByRole('button', { name: 'Use this version' })).toBeEnabled()
  })

  it.each([
    [
      422,
      { code: 'component_version_unusable', answer: 'invalid', reason: 'wrongVersion' },
      'This version cannot be selected: component descriptor cannot be used, because it names a different version from the tag it was found by.',
    ],
    [
      422,
      { code: 'component_version_unusable', answer: 'undescribed' },
      'This version cannot be selected: no component descriptor attached.',
    ],
    [
      422,
      { code: 'component_version_unusable', answer: 'incoherent' },
      'This version cannot be selected: built more than once.',
    ],
    [503, { code: 'registry_unavailable' }, 'The registry could not be asked just now. Try again shortly.'],
    [
      409,
      { code: 'component_version_already_selected' },
      'This component already records this version’s component descriptor. Nothing changed.',
    ],
  ])('words a %i refusal from its answer, beside the server’s own message', async (status, error, lead) => {
    const message = 'The control plane’s own words.'
    serve(server({ change: () => json(status, { error: { ...error, message } }) }))
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Component ID *' }), 'reports')
    const picker = await choose('1.4.0')
    await userEvent.click(within(picker).getByRole('button', { name: 'Use this version' }))

    const alert = await within(picker).findByRole('alert')
    expect(alert).toHaveTextContent(lead)
    expect(alert).toHaveTextContent(message)
    // Worded by the picker alone, not also as the page's save error.
    expect(screen.getAllByText(message)).toHaveLength(1)
  })

  it('offers to reload when the catalogue changed underneath the selection', async () => {
    serve(server({ change: () => json(409, { error: { code: 'revision_conflict', message: 'stale' } }) }))
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Component ID *' }), 'reports')
    const picker = await choose('1.4.0')
    await userEvent.click(within(picker).getByRole('button', { name: 'Use this version' }))

    expect(await within(picker).findByText('The catalogue changed since this page read it.')).toBeInTheDocument()
    expect(within(picker).getByRole('button', { name: 'Reload latest version' })).toBeInTheDocument()
  })

  it('clears a refusal when the catalogue is reloaded', async () => {
    serve(server({ change: () => json(409, { error: { code: 'revision_conflict', message: 'stale' } }) }))
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Component ID *' }), 'reports')
    const picker = await choose('1.4.0')
    await userEvent.click(within(picker).getByRole('button', { name: 'Use this version' }))
    await userEvent.click(await within(picker).findByRole('button', { name: 'Reload latest version' }))

    expect(within(picker).queryByText('The catalogue changed since this page read it.')).not.toBeInTheDocument()
  })

  it.each([
    ['a network failure', () => Promise.reject(new TypeError('Failed to fetch'))],
    ['a gateway’s answer', () => Promise.resolve(new Response('upstream timed out', { status: 504 }))],
    ['the request timing out', () => Promise.resolve(new Response('', { status: 408 }))],
  ])('says the outcome is not known after %s, and offers to reload', async (_, answer) => {
    const state = server()
    serve(state)
    const fetched = vi.mocked(fetch)
    const answering = fetched.getMockImplementation()
    fetched.mockImplementation((url, init) =>
      url === '/api/catalogue' && init?.method === 'POST'
        ? answer()
        : (answering as NonNullable<typeof answering>)(url, init),
    )
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Component ID *' }), 'reports')
    const picker = await choose('1.4.0')
    await userEvent.click(within(picker).getByRole('button', { name: 'Use this version' }))

    const alert = await within(picker).findByRole('alert')
    expect(alert).toHaveTextContent('No answer came back, so whether this version was recorded is not known.')
    expect(alert).not.toHaveTextContent('refused')
    expect(within(alert).getByRole('button', { name: 'Reload latest version' })).toBeInTheDocument()
  })

  it('is disabled for a new component given an ID another component already has', async () => {
    const calls = serve(server({ catalogue: stored(definition([authored('web', 'container')])) }))
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    const ids = screen.getAllByRole('textbox', { name: 'Component ID *' })
    await userEvent.type(ids[ids.length - 1] as HTMLElement, 'web')
    const picker = await choose('1.4.0')

    expect(within(picker).getByRole('button', { name: 'Use this version' })).toBeDisabled()
    expect(within(picker).getByText('Another component already has this ID.')).toBeInTheDocument()
    expect(calls.some((call) => call.method === 'POST')).toBe(false)
  })

  it('offers a registry not read through since the last restart, and lists and selects nothing from it', async () => {
    const calls = serve(server({ listing: listing({ ...REGISTRY, installed: false }) }))
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    const registry = await screen.findByRole('combobox', { name: 'Registry' })
    await userEvent.selectOptions(registry, 'registry.example.com')

    expect(
      within(registry).getByRole('option', {
        name: 'registry.example.com (not read through since the last restart)',
      }),
    ).toBeInTheDocument()
    expect(screen.getByText(/Nothing is read through registry.example.com since the control plane last started/)).toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: 'Repository' })).not.toBeInTheDocument()
    expect(calls.some((call) => call.url === VERSIONS_URL)).toBe(false)
  })

  it('says when nothing is registered, when a registry has no repositories, and when no tag is a version', async () => {
    const state = server({ listing: listing() })
    serve(state)
    const { unmount } = render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    expect(await screen.findByText(/No registries are registered/)).toBeInTheDocument()
    unmount()

    state.listing = listing({ ...REGISTRY, repositories: [] })
    const second = render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.selectOptions(await screen.findByRole('combobox', { name: 'Registry' }), 'registry.example.com')
    expect(screen.getByText('No repositories are registered under registry.example.com.')).toBeInTheDocument()
    second.unmount()

    state.listing = listing(REGISTRY)
    state.versions = () => json(200, { tags: [], other: 1 })
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.selectOptions(await screen.findByRole('combobox', { name: 'Registry' }), 'registry.example.com')
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Repository' }), REPOSITORY)
    expect(await screen.findByText('No tag of this repository is a version.')).toBeInTheDocument()
    expect(screen.getByText('1 tag is not a version and is not listed.')).toBeInTheDocument()
  })

  it('shows a version listing the control plane refused in its own words', async () => {
    serve(
      server({
        versions: () => json(503, { error: { code: 'registry_unavailable', message: 'The registry did not answer.' } }),
      }),
    )
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.selectOptions(await screen.findByRole('combobox', { name: 'Registry' }), 'registry.example.com')
    await userEvent.selectOptions(screen.getByRole('combobox', { name: 'Repository' }), REPOSITORY)

    expect(await screen.findByText(/The version tags could not be listed: The registry did not answer./)).toBeInTheDocument()
  })

  it('converts a saved container in place through the same selection, and fixes a saved kind', async () => {
    const calls = serve(
      server({
        catalogue: stored(definition([authored('web', 'container')])),
        change: () => json(200, stored(definition([described('web', resolution('1.4.0', FIRST))]))),
      }),
    )
    render(<Harness />)
    await openTab('Components')

    expect(screen.queryByRole('combobox', { name: 'Kind' })).not.toBeInTheDocument()
    await userEvent.click(screen.getByRole('button', { name: 'Select a component artifact instead' }))
    const picker = await choose('1.4.0')
    await userEvent.click(within(picker).getByRole('button', { name: 'Use this version' }))

    await waitFor(() => {
      expect(calls.find((call) => call.method === 'POST')?.body).toMatchObject({
        action: 'selectComponentVersion',
        component: 'web',
      })
    })
    expect(await screen.findByRole('region', { name: 'Declared by the component descriptor' })).toBeInTheDocument()
  })
})

describe('the workspace around a described component', () => {
  it('saves a described component without its resolution, reference or version', async () => {
    const calls = serve(
      server({ catalogue: stored(definition([described('reports', resolution('1.4.0', FIRST))])) }),
    )
    render(<Harness />)
    await openTab('Components')
    await userEvent.type(screen.getByRole('textbox', { name: 'Component name *' }), ' v2')
    await userEvent.click(screen.getByRole('button', { name: 'Save draft' }))

    await waitFor(() => {
      expect(calls.some((call) => call.method === 'POST')).toBe(true)
    })
    const post = calls.find((call) => call.method === 'POST')
    expect(post?.body).toMatchObject({ action: 'saveApplication', id: 'analytics' })
    expect((post?.body as { definition: ApplicationDefinition }).definition.components).toEqual([
      { kind: 'described', id: 'reports', name: 'Reports v2', required: false, policy: 'manual' },
    ])
    expect(JSON.stringify(post?.body)).not.toContain('sha256')
  })

  it('saves the other edits without a component waiting for its version, keeps it, and frees the picker', async () => {
    const renamed = { ...definition([]), name: 'Analytics renamed' }
    const calls = serve(server({ change: () => json(200, stored(renamed)) }))
    render(<Harness />)
    await openTab('Components')
    await userEvent.click(screen.getByRole('button', { name: '+ Add component' }))
    await userEvent.type(screen.getByRole('textbox', { name: 'Component ID *' }), 'reports')
    await openTab('Definition')
    await userEvent.type(screen.getByRole('textbox', { name: 'Name *' }), ' renamed')
    await openTab('Components')
    const blocked = await choose('1.4.0')
    expect(within(blocked).getByRole('button', { name: 'Use this version' })).toBeDisabled()

    await userEvent.click(screen.getByRole('button', { name: 'Save draft' }))

    await waitFor(() => {
      expect(calls.some((call) => call.method === 'POST')).toBe(true)
    })
    const post = calls.find((call) => call.method === 'POST')
    expect((post?.body as { definition: ApplicationDefinition }).definition.components).toEqual([])
    expect(await screen.findByRole('textbox', { name: 'Component ID *' })).toHaveValue('reports')
    const picker = await choose('1.4.0')
    expect(within(picker).getByRole('button', { name: 'Use this version' })).toBeEnabled()
    expect(screen.queryByText('Unsaved changes')).not.toBeInTheDocument()
  })

  it('calls a draft equal to its release published, resolutions included', async () => {
    const draft = definition([described('reports', resolution('1.4.0', FIRST))])
    serve(server({ catalogue: stored(draft, [{ version: 1, note: '', publishedAt: 0, definition: draft }]) }))
    render(<Harness />)

    expect(await screen.findByText('Published')).toBeInTheDocument()
  })

  it('renders every tab for a server draft with no resources key, declared content read-only and labelled', async () => {
    const draft = definition([
      authored('web', 'container'),
      described('reports', resolution('1.4.0', FIRST)),
      { ...described('audit', resolution('2.0.0', SECOND)), name: 'Audit' },
    ])
    expect(draft.resources).toBeUndefined()
    serve(server({ catalogue: stored(draft) }))
    render(<Harness />)

    for (const tab of APPLICATION_TABS) {
      await openTab(tab)
    }

    await openTab('Resources')
    expect(screen.getByText('No resources yet.')).toBeInTheDocument()
    const reports = screen.getByRole('region', { name: 'Declared by Reports' })
    expect(within(reports).getByTitle(FIRST)).toHaveTextContent('sha256:333333333333…')
    // The resource's name and its collection.
    expect(within(reports).getAllByText('reports', { selector: 'td' })).toHaveLength(2)
    expect(within(reports).getByText('primary')).toBeInTheDocument()
    expect(within(reports).getByText('read, list')).toBeInTheDocument()
    expect(screen.getByRole('region', { name: 'Declared by Audit' })).toBeInTheDocument()
    expect(screen.queryByRole('textbox', { name: 'Resource name *' })).not.toBeInTheDocument()

    await openTab('Client configuration')
    const fields = screen.getByRole('region', { name: 'Declared by Reports' })
    expect(within(fields).getByTitle(FIRST)).toHaveTextContent('sha256:333333333333…')
    expect(within(fields).getByText('team')).toBeInTheDocument()
    expect(within(fields).getByText('Required', { selector: 'td' })).toBeInTheDocument()
    expect(within(fields).getByText('No default')).toBeInTheDocument()
  })
})
