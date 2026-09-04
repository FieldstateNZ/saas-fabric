/**
 * Choosing something the environment ran before.
 *
 * The contract under test is what an operator *cannot* do: name a version the
 * platform did not observe, or influence which digests get written.
 */
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { RollbackPicker } from './RollbackPicker'

function offering(versions: { version: string; source_revision?: string }[], more = false) {
  const fetched = vi.fn().mockImplementation((url: string) => {
    if (url.endsWith('/versions')) {
      return Promise.resolve({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ versions, more }),
      })
    }

    return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve({}) })
  })

  vi.stubGlobal('fetch', fetched)
  vi.stubGlobal('location', { reload: vi.fn() })

  return fetched
}

const EARLIER = [
  { version: '0.3.0-preview.4', source_revision: 'b08ba9e1234567' },
  { version: '0.3.0-preview.3', source_revision: 'f20d6dc7654321' },
]

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('the rollback picker', () => {
  it('offers only what the platform observed, and nothing to type it into', async () => {
    offering(EARLIER)

    render(<RollbackPicker component="saas-fabric" artifact="oci" onCancel={() => undefined} />)

    expect(await screen.findByRole('button', { name: '0.3.0-preview.4' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '0.3.0-preview.3' })).toBeInTheDocument()

    // One text field, and it is the note. There is nowhere to name a version.
    const fields = screen.getAllByRole('textbox')
    expect(fields).toHaveLength(1)
    expect(fields[0]).toHaveAttribute('id', 'rollback-note-saas-fabric')
  })

  it('sends a version and a note, and never a digest', async () => {
    const fetched = offering(EARLIER)

    render(<RollbackPicker component="saas-fabric" artifact="oci" onCancel={() => undefined} />)
    await userEvent.click(await screen.findByRole('button', { name: '0.3.0-preview.4' }))

    const post = fetched.mock.calls.find(([url]) => String(url).endsWith('/rollback'))
    const [url, options] = post as [string, { method: string; body?: string }]

    expect(url).toBe('/api/platform/components/saas-fabric/rollback')
    expect(options.method).toBe('POST')

    const body = JSON.parse(options.body ?? '{}') as Record<string, unknown>

    // The digests are the platform's to resolve at the moment of the write.
    // A browser that could send one could send a wrong one.
    expect(Object.keys(body).sort()).toEqual(['note', 'version'])
    expect(body.version).toBe('0.3.0-preview.4')
  })

  it('says when older versions exist that it did not list', async () => {
    // A list that stopped quietly would read as "this is everything there is".
    offering(EARLIER, true)

    render(<RollbackPicker component="saas-fabric" artifact="oci" onCancel={() => undefined} />)

    expect(await screen.findByText(/older versions exist and are not listed/i)).toBeInTheDocument()
  })

  it('says so plainly when there is nowhere to go back to', async () => {
    offering([])

    render(<RollbackPicker component="saas-fabric" artifact="oci" onCancel={() => undefined} />)

    expect(await screen.findByText(/has not run an earlier version/i)).toBeInTheDocument()
    expect(screen.queryByRole('textbox')).not.toBeInTheDocument()
  })
})

describe('a chart component', () => {
  const CHART_VERSIONS = [{ version: '7.3.0' }, { version: '7.2.3' }]

  it('lists candidates that carry no source revision', async () => {
    // A chart repository's index lists versions and no provenance, so the API
    // omits the field. Rendering "built from" with nothing after it would say
    // the platform looked for a commit and found none, which is a different
    // claim from there being no commit to look for.
    offering(CHART_VERSIONS)

    render(<RollbackPicker component="keycloak" artifact="helm" onCancel={() => undefined} />)

    expect(await screen.findByRole('button', { name: '7.3.0' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: '7.2.3' })).toBeInTheDocument()
    expect(screen.queryByText(/built from/i)).not.toBeInTheDocument()
  })

  it('says what rolling a chart back restores, and what it does not', async () => {
    // The caveat the decision turns on. The operator gets the capability *and*
    // an accurate account of it, rather than being refused in the name of a
    // guarantee they were never promised.
    offering(CHART_VERSIONS)

    render(<RollbackPicker component="keycloak" artifact="helm" onCancel={() => undefined} />)

    expect(await screen.findByText(/restores the chart version/i)).toBeInTheDocument()
    expect(
      screen.getByText(/not the byte-for-byte return an image rollback is/i),
    ).toBeInTheDocument()
  })

  it('sends the same body an image rollback does', async () => {
    // One request shape for both kinds. A version and a note; the release the
    // platform writes is the platform's to resolve.
    const fetched = offering(CHART_VERSIONS)

    render(<RollbackPicker component="keycloak" artifact="helm" onCancel={() => undefined} />)
    await userEvent.click(await screen.findByRole('button', { name: '7.2.3' }))

    const post = fetched.mock.calls.find(([url]) => String(url).endsWith('/rollback'))
    const [url, options] = post as [string, { method: string; body?: string }]

    expect(url).toBe('/api/platform/components/keycloak/rollback')
    expect(options.method).toBe('POST')

    const body = JSON.parse(options.body ?? '{}') as Record<string, unknown>

    expect(Object.keys(body).sort()).toEqual(['note', 'version'])
    expect(body.version).toBe('7.2.3')
  })
})
