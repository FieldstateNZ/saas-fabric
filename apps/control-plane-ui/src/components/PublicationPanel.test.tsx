/**
 * The runtime-publication panel, from an operator's side.
 *
 * What these pin: `null` documents read as "not reported" rather than zero,
 * each revision is bound to its own row rather than merely present
 * somewhere on the page, a refused click is told apart from a fault, and
 * the trigger renders the row its own call produced rather than asking the
 * operator to reload.
 */
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Publication } from '../api/publication-types'
import { PublicationPanel } from './PublicationPanel'

function publication(overrides: Partial<Publication> = {}): Publication {
  return {
    target: 'ConfigMaps in namespace platform-system',
    documents: { tenants: null, dataSources: null, catalog: null },
    lastPass: null,
    ...overrides,
  }
}

/**
 * The `<dd>` beside the `<dt>` named `label`.
 *
 * `getAllByText('—')` and its like only prove a value appears somewhere; a
 * swapped `tenants`/`dataSources` mapping in `PublicationRows` would still
 * pass. Reading the value next to its own label is the assertion that
 * actually pins which fact is which.
 */
function fact(label: string): Element | null {
  return screen.getByText(label).nextElementSibling
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('not configured', () => {
  it('says so plainly, and offers no control', () => {
    render(<PublicationPanel publication={null} />)

    expect(screen.getByText(/not configured for this deployment/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /publish now/i })).not.toBeInTheDocument()
  })
})

describe('a configured target', () => {
  it('says never and shows a dash for every document before a first pass', () => {
    render(<PublicationPanel publication={publication()} />)

    expect(screen.getByText('Never')).toBeInTheDocument()
    // Not `0`, and not "none": nothing has run, so nothing has been reported.
    expect(fact('Tenants')).toHaveTextContent(/^—$/)
    expect(fact('Data sources')).toHaveTextContent(/^—$/)
    expect(fact('Catalogue')).toHaveTextContent(/^—$/)
    expect(screen.getByRole('button', { name: 'Publish now' })).toBeInTheDocument()
    // The panel never claims anything about the runtime itself.
    expect(screen.getByText(/not whether the runtime has reloaded/)).toBeInTheDocument()
  })

  it('shows the revisions a published pass reported, and the word Published', () => {
    render(
      <PublicationPanel
        publication={publication({
          documents: { tenants: 3, dataSources: 2, catalog: 1 },
          lastPass: { atUnixSeconds: 1_700_000_000, outcome: 'published', detail: null },
        })}
      />,
    )

    expect(fact('Tenants')).toHaveTextContent(/^revision 3$/)
    expect(fact('Data sources')).toHaveTextContent(/^revision 2$/)
    expect(fact('Catalogue')).toHaveTextContent(/^revision 1$/)
    expect(screen.getByText('Published')).toBeInTheDocument()
  })

  it('shows why a pass is waiting, and a dash rather than a stale revision', () => {
    render(
      <PublicationPanel
        publication={publication({
          // A `waiting` pass never reaches a read of `current()` worth
          // reporting as held, so the API sends every document `null` --
          // the fixture states that directly rather than relying on the
          // panel to infer it from the outcome, which is not its job.
          documents: { tenants: null, dataSources: null, catalog: null },
          lastPass: {
            atUnixSeconds: 1_700_000_000,
            outcome: 'waiting',
            detail: 'the derived runtime catalogue has no resources yet',
          },
        })}
      />,
    )

    expect(screen.getByText('Waiting')).toBeInTheDocument()
    expect(screen.getByText('Waiting')).toHaveClass('status--waiting')
    expect(screen.getByText(/no resources yet/)).toBeInTheDocument()
    expect(fact('Tenants')).toHaveTextContent(/^—$/)
    expect(fact('Data sources')).toHaveTextContent(/^—$/)
    expect(fact('Catalogue')).toHaveTextContent(/^—$/)
  })

  it('shows the word and the detail for a refused pass', () => {
    render(
      <PublicationPanel
        publication={publication({
          lastPass: {
            atUnixSeconds: 1_700_000_000,
            outcome: 'refused',
            detail: 'no platform repository is connected yet',
          },
        })}
      />,
    )

    expect(screen.getByText('Refused')).toBeInTheDocument()
    expect(screen.getByText('Refused')).toHaveClass('status--refused')
    expect(screen.getByText(/no platform repository is connected/)).toBeInTheDocument()
  })

  it('shows the word and the detail for a failed pass', () => {
    render(
      <PublicationPanel
        publication={publication({
          lastPass: {
            atUnixSeconds: 1_700_000_000,
            outcome: 'failed',
            detail: 'the publication target is unavailable',
          },
        })}
      />,
    )

    expect(screen.getByText('Failed')).toBeInTheDocument()
    expect(screen.getByText(/publication target is unavailable/)).toBeInTheDocument()
  })
})

describe('publishing now', () => {
  it('sends no body, and renders the row the pass produced instead of reloading', async () => {
    const fetched = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () =>
        Promise.resolve({
          target: 'ConfigMaps in namespace platform-system',
          documents: { tenants: 9, dataSources: 8, catalog: 7 },
          lastPass: { atUnixSeconds: 1_700_000_500, outcome: 'published', detail: null },
        }),
    })
    vi.stubGlobal('fetch', fetched)
    const reload = vi.fn()
    vi.stubGlobal('location', { reload })

    render(<PublicationPanel publication={publication()} />)
    await userEvent.click(screen.getByRole('button', { name: 'Publish now' }))

    // The stubbed response carries revisions the initial prop did not, each
    // bound to its own row -- seeing them proves the panel rendered the
    // trigger's own answer, correctly laid out, not just re-displayed what
    // it started with.
    await screen.findByText('revision 9')
    expect(fact('Tenants')).toHaveTextContent(/^revision 9$/)
    expect(fact('Data sources')).toHaveTextContent(/^revision 8$/)
    expect(fact('Catalogue')).toHaveTextContent(/^revision 7$/)

    // The success path speaks too, not only the row: a screen reader that
    // was not looking at the facts list still hears that the pass finished.
    expect(screen.getByRole('status')).toHaveTextContent(/published/i)

    const [url, options] = fetched.mock.calls[0] as [string, { method: string; body?: string }]

    expect(url).toBe('/api/platform/publication')
    expect(options.method).toBe('POST')
    expect(options.body).toBeUndefined()
    expect(reload).not.toHaveBeenCalled()
  })

  it('announces why, not only that, when the pass it ran did not publish', async () => {
    // A screen-reader user hears the live region and not the pill; "Failed"
    // alone would send them hunting through the facts list for the reason
    // the row already carries.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({
            target: 'ConfigMaps in namespace platform-system',
            documents: { tenants: null, dataSources: null, catalog: null },
            lastPass: {
              atUnixSeconds: 1_700_000_500,
              outcome: 'failed',
              detail: 'the publication target is unavailable',
            },
          }),
      }),
    )

    render(<PublicationPanel publication={publication()} />)
    await userEvent.click(screen.getByRole('button', { name: 'Publish now' }))

    expect(await screen.findByRole('status')).toHaveTextContent(
      /^Pass finished — Failed — the publication target is unavailable$/,
    )
  })

  it('treats a pass already running as a refusal of the click, not a fault', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 409,
        json: () =>
          Promise.resolve({
            error: { code: 'publication_running', message: 'a pass is already running' },
          }),
      }),
    )

    render(<PublicationPanel publication={publication()} />)
    await userEvent.click(screen.getByRole('button', { name: 'Publish now' }))

    expect(await screen.findByRole('status')).toHaveTextContent(/already running/i)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Publish now' })).not.toBeDisabled()
  })

  it('shows any other refusal as an alert', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 503,
        json: () =>
          Promise.resolve({
            error: { code: 'repository_unavailable', message: 'the platform repository is unavailable' },
          }),
      }),
    )

    render(<PublicationPanel publication={publication()} />)
    await userEvent.click(screen.getByRole('button', { name: 'Publish now' }))

    expect(await screen.findByRole('alert')).toHaveTextContent(/repository is unavailable/i)
  })

  it('clears a stale notice and adopts a fresher prop when the parent keeps the panel mounted', async () => {
    // This is not how today's `usePlatform` behaves -- it blanks `value`
    // across a refresh, which unmounts this panel instead. It pins the
    // effect that exists for whichever caller does not: a notice about a
    // pass this panel ran must not survive being handed a different row.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 409,
        json: () =>
          Promise.resolve({
            error: { code: 'publication_running', message: 'a pass is already running' },
          }),
      }),
    )

    const { rerender } = render(<PublicationPanel publication={publication()} />)
    await userEvent.click(screen.getByRole('button', { name: 'Publish now' }))
    expect(await screen.findByRole('status')).toHaveTextContent(/already running/i)

    rerender(
      <PublicationPanel
        publication={publication({ documents: { tenants: 5, dataSources: 4, catalog: 3 } })}
      />,
    )

    expect(fact('Tenants')).toHaveTextContent(/^revision 5$/)
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
  })
})
