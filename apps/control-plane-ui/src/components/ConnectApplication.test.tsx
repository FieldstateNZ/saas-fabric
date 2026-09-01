/**
 * What the console offers at each step of creating an application.
 *
 * The component is now used by both integrations, so what these pin includes
 * the part that must not be shared: it acts on the endpoints it was handed and
 * has no way to reach the other set.
 */
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { clientConfiguration, platformManagement } from '../api/client'
import type { Application } from '../api/types'
import { ConnectApplication } from './ConnectApplication'

function connect(application: Application | null = null) {
  return (
    <ConnectApplication
      endpoints={clientConfiguration}
      application={application}
      purpose="the clients this platform manages"
    />
  )
}

afterEach(() => {
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

describe('the connect panel', () => {
  it('asks for an organisation before it will create anything', async () => {
    render(connect())

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

    render(connect())
    await userEvent.type(screen.getByLabelText(/organisation/i), 'FieldstateNZ')
    await userEvent.click(screen.getByRole('button', { name: /create application/i }))

    expect(submit).toHaveBeenCalled()

    const form = document.querySelector('form')
    expect(form?.method).toBe('post')
    expect(form?.action).toBe('https://host.test/new')
    expect(form?.querySelector('input')?.name).toBe('manifest')
  })

  it('offers the install step once an application exists', () => {
    render(connect({ slug: 'saas-fabric', account: null, installed: false, repository: null }))

    expect(screen.getByRole('button', { name: /install on github/i })).toBeInTheDocument()
    expect(screen.queryByLabelText(/organisation/i)).not.toBeInTheDocument()
  })

  it('creates the application its endpoints name, and no other', async () => {
    const fetched = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ post_url: 'https://host.test/new', manifest: {} }),
    })
    vi.stubGlobal('fetch', fetched)
    vi.spyOn(HTMLFormElement.prototype, 'submit').mockImplementation(() => undefined)

    render(
      <ConnectApplication
        endpoints={platformManagement}
        application={null}
        purpose="this platform's own composition"
      />,
    )
    await userEvent.type(screen.getByLabelText(/organisation/i), 'FieldstateNZ')
    await userEvent.click(screen.getByRole('button', { name: /create application/i }))

    // The platform panel cannot create the client application, because it was
    // never handed a way to ask for one.
    expect(fetched).toHaveBeenCalledWith(
      '/api/integrations/platform/connect',
      expect.anything(),
    )
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

    render(connect())
    await userEvent.type(screen.getByLabelText(/organisation/i), 'FieldstateNZ')
    await userEvent.click(screen.getByRole('button', { name: /create application/i }))

    expect(await screen.findByText(/The Git host refused\./)).toBeInTheDocument()
  })
})
