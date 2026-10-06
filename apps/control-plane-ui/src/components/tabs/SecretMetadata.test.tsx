/**
 * Reading a secret's version on demand, and what that read may and may not do.
 *
 * Rendered through the real `Secrets` tab and the real API client, with only
 * `fetch` replaced. The replies are held in the test's hand and released one at
 * a time, so every ordering below is the one the test says it is -- no timers,
 * no sleeps. Anything the console shows after a reply is awaited with Testing
 * Library's async queries, which is what makes the same file pass under
 * Vitest 3 and 4 without either knowing how many microtask hops a reply takes.
 */
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import type { Client } from '../../api/types'
import { Secrets } from './Secrets'

const ACME: Client = { id: 'acme', displayName: 'Acme', hosts: [], realm: 'acme', revision: 'a' }
const BETA: Client = { id: 'beta', displayName: 'Beta', hosts: [], realm: 'beta', revision: 'b' }

const PATH = 'database/primary'
const UNUSABLE = 'The control plane answered with a version this console cannot carry.'

/**
 * A reply the test releases when it chooses.
 *
 * `body` absent means the response has no body at all, as a real `500` from a
 * proxy might; `body: null` means the response is the JSON text `null`. The two
 * are different on the wire and stay different here.
 */
interface Reply {
  readonly status: number
  readonly body?: unknown
}

/** One request the console made that is still waiting on the test. */
interface Held {
  readonly url: string
  readonly body: unknown
  readonly answer: (reply: Reply) => void
  /** Settles once the console has asked the response for its body. */
  readonly consumed: Promise<void>
}

/** What the console sent, by method. */
interface Call {
  readonly url: string
  readonly method: string
}

/**
 * A control plane that lists one secret at once and holds everything else.
 *
 * Listings answer immediately so a tab reaches its table; metadata reads and
 * writes are queued for the test to answer in whatever order it is proving.
 */
function controlPlane() {
  const calls: Call[] = []
  const reads: Held[] = []
  const writes: Held[] = []

  function reply({ status, body }: Reply, onConsumed: () => void = () => {}) {
    return {
      ok: status < 400,
      status,
      json: () => {
        onConsumed()

        // No body is a parse failure, as it is on a real `Response`. `null`
        // is a body, and is handed over as itself: `body ?? {}` here would
        // have hidden the very answer the null-metadata tests are about.
        return body === undefined
          ? Promise.reject(new SyntaxError('Unexpected end of JSON input'))
          : Promise.resolve(body)
      },
    }
  }

  function hold(into: Held[], url: string, init?: RequestInit) {
    return new Promise<ReturnType<typeof reply>>((resolve) => {
      const raw = init?.body
      let markConsumed = () => {}
      const consumed = new Promise<void>((done) => {
        markConsumed = done
      })

      into.push({
        url,
        body: typeof raw === 'string' ? JSON.parse(raw) : null,
        answer: (held) => {
          resolve(reply(held, markConsumed))
        },
        consumed,
      })
    })
  }

  vi.stubGlobal(
    'fetch',
    vi.fn((url: string, init?: RequestInit) => {
      const method = init?.method ?? 'GET'
      calls.push({ url, method })

      if (url.endsWith('/secrets') && method === 'GET') {
        return Promise.resolve(reply({ status: 200, body: [{ path: PATH }] }))
      }
      if (url.includes('/secrets/entry/') && method === 'GET') {
        return hold(reads, url, init)
      }
      if (url.includes('/secrets/entry/') && method === 'PUT') {
        return hold(writes, url, init)
      }

      return Promise.resolve(reply({ status: 204 }))
    }),
  )

  return { calls, reads, writes }
}

/**
 * Releases a held reply and lets React take it in.
 *
 * Awaits the point at which the console has taken the body -- a thing that
 * happens, not a number of ticks guessed at -- inside `act`, so React flushes
 * whatever that caused. What the console then shows is asserted with
 * `findBy` / `waitFor` by the caller, never assumed to be there already.
 */
async function release(held: Held | undefined, reply: Reply): Promise<void> {
  expect(held).toBeDefined()
  if (held === undefined) {
    return
  }

  await act(async () => {
    held.answer(reply)
    await held.consumed
  })
}

async function renderTab(client: Client = ACME) {
  const rendered = render(<Secrets client={client} />)
  await screen.findByText(PATH)

  return rendered
}

const readButton = () => screen.getByRole('button', { name: `Read version of ${PATH}` })
const refreshButton = () => screen.getByRole('button', { name: `Refresh version of ${PATH}` })
const retryButton = () => screen.getByRole('button', { name: `Retry reading version of ${PATH}` })
const shownVersion = (version: string) => screen.queryByText(version, { selector: 'code' })
const findShownVersion = (version: string) => screen.findByText(version, { selector: 'code' })

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('reading a secret version', () => {
  it('shows it is reading, refuses a second read meanwhile, then shows the version', async () => {
    const plane = controlPlane()
    await renderTab()

    const status = screen.getByRole('status')
    expect(status).toBeEmptyDOMElement()

    await userEvent.click(readButton())

    expect(screen.getByRole('status')).toBe(status)
    const reading = screen.getByRole('button', { name: `Reading version of ${PATH}` })
    expect(reading).toBeDisabled()
    expect(status).toHaveTextContent('Reading version…')

    await userEvent.click(reading)
    expect(plane.reads).toHaveLength(1)

    await release(plane.reads[0], { status: 200, body: { version: 7, updatedAt: null } })

    expect(await findShownVersion('7')).toBeInTheDocument()
    expect(refreshButton()).toBeEnabled()
    expect(screen.getByRole('status')).toBe(status)
    expect(status).toHaveTextContent(`Observed version 7 of ${PATH}`)
  })

  it('admits one read even when clicked twice in one tick', async () => {
    const plane = controlPlane()
    await renderTab()

    const button = readButton()
    await act(async () => {
      button.click()
      button.click()
    })

    expect(plane.reads).toHaveLength(1)
  })

  it('reads by GET on the entry and never reveals or posts anything', async () => {
    const plane = controlPlane()
    await renderTab()

    await userEvent.click(readButton())
    await release(plane.reads[0], {
      status: 200,
      body: { version: 7, updatedAt: '2026-10-05T01:02:03Z' },
    })

    expect(await findShownVersion('7')).toBeInTheDocument()
    expect(screen.getByText(/updated 2026-10-05T01:02:03Z/)).toBeInTheDocument()
    expect(plane.reads[0]?.url).toBe('/api/clients/acme/secrets/entry/database/primary')

    // Still hidden: a version read must not become a reveal by accident.
    expect(screen.getByText('hidden')).toBeInTheDocument()
    expect(plane.calls.filter((call) => call.method !== 'GET')).toHaveLength(0)
    expect(plane.calls.some((call) => call.url.endsWith('/secrets/reveal'))).toBe(false)
  })

  it('shows a failure and reads again only when asked', async () => {
    const plane = controlPlane()
    await renderTab()

    await userEvent.click(readButton())
    await release(plane.reads[0], {
      status: 503,
      body: { error: { code: 'store_unavailable', message: 'The secret store is away.' } },
    })

    expect(await screen.findByText('The secret store is away.')).toHaveAttribute('role', 'alert')
    expect(plane.reads).toHaveLength(1)

    await userEvent.click(retryButton())
    expect(plane.reads).toHaveLength(2)
    await release(plane.reads[1], { status: 200, body: { version: 3, updatedAt: null } })

    expect(await findShownVersion('3')).toBeInTheDocument()
    expect(screen.queryByText('The secret store is away.')).toBeNull()
  })

  it('drops the old version while a fresh read is pending, and keeps it dropped when that fails', async () => {
    const plane = controlPlane()
    await renderTab()

    await userEvent.click(readButton())
    await release(plane.reads[0], { status: 200, body: { version: 7, updatedAt: null } })
    expect(await findShownVersion('7')).toBeInTheDocument()

    await userEvent.click(refreshButton())
    expect(shownVersion('7')).toBeNull()

    await release(plane.reads[1], { status: 500 })
    expect(await screen.findByText('The control plane answered 500.')).toBeInTheDocument()
    expect(shownVersion('7')).toBeNull()
    expect(retryButton()).toBeEnabled()
  })

  it('discards an answer that arrives after the row has gone', async () => {
    const plane = controlPlane()
    const { unmount } = await renderTab()

    await userEvent.click(readButton())
    unmount()

    await release(plane.reads[0], { status: 200, body: { version: 7, updatedAt: null } })
    expect(shownVersion('7')).toBeNull()
  })

  it("never lets one client's answer land on another client's row", async () => {
    const plane = controlPlane()
    const { rerender } = await renderTab(ACME)

    await userEvent.click(readButton())
    expect(plane.reads[0]?.url).toContain('/api/clients/acme/')

    rerender(<Secrets client={BETA} />)
    await screen.findByText(PATH)
    expect(plane.calls.filter((call) => call.url === '/api/clients/beta/secrets')).toHaveLength(1)

    await release(plane.reads[0], { status: 200, body: { version: 7, updatedAt: null } })

    expect(shownVersion('7')).toBeNull()
    expect(readButton()).toBeEnabled()
    expect(plane.reads).toHaveLength(1)
  })

  it('shows an actionable error for a null metadata response, and a retry can then succeed', async () => {
    const plane = controlPlane()
    await renderTab()

    await userEvent.click(readButton())
    // A `200` whose body is the JSON text `null`: the type says metadata, the
    // wire says nothing. This must land as a failure the operator can act on,
    // not as a rejection nobody is waiting for with the row stuck reading.
    await release(plane.reads[0], { status: 200, body: null })

    expect(await screen.findByText(UNUSABLE)).toHaveAttribute('role', 'alert')
    expect(screen.queryByText('Reading version…')).toBeNull()
    expect(document.querySelector('code')).toBeNull()
    expect(retryButton()).toBeEnabled()

    // Nothing was revealed or written on the way to that error.
    expect(screen.getByText('hidden')).toBeInTheDocument()
    expect(plane.calls.filter((call) => call.method !== 'GET')).toHaveLength(0)

    await userEvent.click(retryButton())
    expect(plane.reads).toHaveLength(2)
    await release(plane.reads[1], { status: 200, body: { version: 5, updatedAt: null } })

    expect(await findShownVersion('5')).toBeInTheDocument()
    expect(screen.queryByText(UNUSABLE)).toBeNull()
    expect(refreshButton()).toBeEnabled()
  })

  it.each([
    ['an array', [7]],
    ['a bare string', 'seven'],
    ['a bare number', 7],
    ['a boolean', true],
  ])('refuses a metadata body that is %s', async (_, body) => {
    const plane = controlPlane()
    await renderTab()

    await userEvent.click(readButton())
    await release(plane.reads[0], { status: 200, body })

    expect(await screen.findByText(UNUSABLE)).toBeInTheDocument()
    expect(document.querySelector('code')).toBeNull()
    expect(retryButton()).toBeEnabled()
  })

  it.each([
    ['beyond a safe integer', 2 ** 53],
    ['negative', -1],
    ['a string', '7'],
    ['fractional', 7.5],
    ['missing', undefined],
  ])('refuses to show a version that is %s', async (_, version) => {
    const plane = controlPlane()
    await renderTab()

    await userEvent.click(readButton())
    await release(plane.reads[0], { status: 200, body: { version, updatedAt: null } })

    expect(await screen.findByText(UNUSABLE)).toBeInTheDocument()
    expect(document.querySelector('code')).toBeNull()
    expect(retryButton()).toBeEnabled()
  })
})

describe('replacing a secret somebody else has moved', () => {
  it('keeps the draft on conflict, refreshes only when asked, and writes what the operator typed', async () => {
    const plane = controlPlane()
    await renderTab()

    await userEvent.click(readButton())
    await release(plane.reads[0], { status: 200, body: { version: 7, updatedAt: null } })
    expect(await findShownVersion('7')).toBeInTheDocument()

    // The operator copies the observed version by hand. Nothing fills it in.
    expect(screen.getByLabelText('Replacing version')).toHaveValue('')
    await userEvent.type(screen.getByLabelText('Path'), PATH)
    await userEvent.type(screen.getByLabelText('Key'), 'password')
    await userEvent.type(screen.getByLabelText('Value'), 'synthetic-replacement-value')
    await userEvent.type(screen.getByLabelText('Replacing version'), '7')
    await userEvent.click(screen.getByRole('button', { name: 'Write' }))

    await waitFor(() => {
      expect(plane.writes).toHaveLength(1)
    })
    expect(plane.writes[0]?.body).toMatchObject({ expectedVersion: 7 })

    // The server moved to 8 in the meantime.
    await release(plane.writes[0], {
      status: 409,
      body: { error: { code: 'version_conflict', message: 'database/primary is at version 8, not 7.' } },
    })

    expect(await screen.findByText('database/primary is at version 8, not 7.')).toBeInTheDocument()
    expect(screen.getByLabelText('Path')).toHaveValue(PATH)
    expect(screen.getByLabelText('Key')).toHaveValue('password')
    expect(screen.getByLabelText('Replacing version')).toHaveValue('7')
    // No retry on the operator's behalf, and no read they did not ask for.
    expect(plane.writes).toHaveLength(1)
    expect(plane.reads).toHaveLength(1)

    await userEvent.click(refreshButton())
    expect(shownVersion('7')).toBeNull()
    await release(plane.reads[1], { status: 200, body: { version: 8, updatedAt: null } })

    expect(await findShownVersion('8')).toBeInTheDocument()
    // Observing 8 changes nothing the operator typed.
    expect(screen.getByLabelText('Replacing version')).toHaveValue('7')

    await userEvent.clear(screen.getByLabelText('Replacing version'))
    await userEvent.type(screen.getByLabelText('Replacing version'), '8')
    await userEvent.click(screen.getByRole('button', { name: 'Write' }))

    await waitFor(() => {
      expect(plane.writes).toHaveLength(2)
    })
    expect(plane.writes[1]?.body).toMatchObject({ expectedVersion: 8 })
    await release(plane.writes[1], { status: 200, body: { version: 9 } })

    await screen.findByText('Wrote database/primary.')
    await screen.findByText(PATH)

    // The listing after a write starts afresh: nothing observed is carried over.
    expect(shownVersion('8')).toBeNull()
    expect(readButton()).toBeEnabled()
    expect(plane.reads).toHaveLength(2)
  })
})
