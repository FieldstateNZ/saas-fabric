/**
 * What the console offers, and what it refuses to offer.
 *
 * The case worth pinning is the deployment that states its own repository: it
 * must not be shown a control that would overwrite that from a browser.
 */
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Integration } from '../api/types'
import { GitIntegration } from './GitIntegration'

function integration(overrides: Partial<Integration>): Integration {
  return {
    status: 'not_configured',
    connection: null,
    last_success_at: null,
    managed: true,
    application: null,
    ...overrides,
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('the connect panel', () => {
  it('offers nothing when the deployment states its own repository', () => {
    const { container } = render(
      <GitIntegration integration={integration({ managed: false })} />,
    )

    expect(container).toBeEmptyDOMElement()
  })

  it('asks for an organisation before it will create anything', async () => {
    render(<GitIntegration integration={integration({})} />)

    const button = screen.getByRole('button', { name: /create application/i })
    expect(button).toBeDisabled()

    await userEvent.type(screen.getByLabelText(/organisation/i), 'FieldstateNZ')
    expect(button).toBeEnabled()
  })

  it('posts the manifest as a real form, because a manifest is a POST', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () =>
          Promise.resolve({ post_url: 'https://host.test/new', manifest: { name: 'SaaS Fabric' } }),
      }),
    )
    const submit = vi.spyOn(HTMLFormElement.prototype, 'submit').mockImplementation(() => undefined)

    render(<GitIntegration integration={integration({})} />)
    await userEvent.type(screen.getByLabelText(/organisation/i), 'FieldstateNZ')
    await userEvent.click(screen.getByRole('button', { name: /create application/i }))

    expect(submit).toHaveBeenCalled()

    const form = document.querySelector('form')
    expect(form?.method).toBe('post')
    expect(form?.action).toBe('https://host.test/new')
    expect(form?.querySelector('input')?.name).toBe('manifest')
  })

  it('offers the install step once an application exists', () => {
    render(
      <GitIntegration
        integration={integration({
          application: { slug: 'saas-fabric', account: null, installed: false, repository: null },
        })}
      />,
    )

    expect(screen.getByRole('button', { name: /install on github/i })).toBeInTheDocument()
    expect(screen.queryByLabelText(/organisation/i)).not.toBeInTheDocument()
  })

  it('shows what went wrong rather than navigating away', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 502,
        json: () =>
          Promise.resolve({ error: { code: 'git_host_refused', message: 'The Git host refused.' } }),
      }),
    )

    render(<GitIntegration integration={integration({})} />)
    await userEvent.type(screen.getByLabelText(/organisation/i), 'FieldstateNZ')
    await userEvent.click(screen.getByRole('button', { name: /create application/i }))

    expect(await screen.findByText(/The Git host refused\./)).toBeInTheDocument()
  })
})
