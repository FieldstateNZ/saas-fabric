/**
 * Where each registry call goes, and what it carries.
 *
 * The routes are fixed; what varies is a host that may carry a port and a
 * repository path of several segments, and both must arrive as the path
 * segments the control plane expects -- never resolved by the browser into
 * a different route.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'

import {
  addRegistryRepository,
  registerRegistry,
  registryVersions,
  removeRegistry,
  removeRegistryCredential,
  removeRegistryRepository,
  setRegistryCredential,
  underHost,
} from './registries'

interface Sent {
  readonly url: string
  readonly method: string
  readonly body: unknown
}

function record(status = 200, answer: unknown = {}): Sent[] {
  const sent: Sent[] = []

  vi.stubGlobal(
    'fetch',
    vi.fn((url: string, init?: RequestInit) => {
      sent.push({
        url,
        method: init?.method ?? 'GET',
        body: typeof init?.body === 'string' ? JSON.parse(init.body) : undefined,
      })

      return Promise.resolve(
        status === 204
          ? new Response(null, { status })
          : new Response(JSON.stringify(answer), {
              status,
              headers: { 'Content-Type': 'application/json' },
            }),
      )
    }),
  )

  return sent
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('underHost', () => {
  it('takes everything after the host as the path', () => {
    expect(underHost('ghcr.io', 'ghcr.io/fieldstatenz/saas-fabric')).toEqual({
      path: 'fieldstatenz/saas-fabric',
    })
    expect(underHost('registry.example.com:5000', 'registry.example.com:5000/acme/reports')).toEqual({
      path: 'acme/reports',
    })
  })

  it('refuses a repository that does not start with the host, naming the prefix', () => {
    for (const typed of ['fieldstatenz/saas-fabric', 'docker.io/library/nginx', 'ghcr.io', 'ghcr.io/', 'ghcr.iox/a']) {
      const place = underHost('ghcr.io', typed)

      expect(place).toEqual({ problem: expect.stringContaining('ghcr.io/') as unknown })
    }
  })

  it('refuses a segment a browser would resolve away', () => {
    for (const typed of ['ghcr.io/../credential', 'ghcr.io/acme/./x', 'ghcr.io/acme//x', 'ghcr.io/acme/']) {
      expect(underHost('ghcr.io', typed)).toHaveProperty('problem')
    }
  })
})

describe('the registry routes', () => {
  it('registers with the body it was given, and nothing else', async () => {
    const sent = record(201)

    await registerRegistry({ kind: 'ghcr' })

    expect(sent).toEqual([
      { url: '/api/integrations/registries', method: 'POST', body: { kind: 'ghcr' } },
    ])
  })

  it('names a host with a port as one encoded segment', async () => {
    const sent = record(204)

    await removeRegistry('registry.example.com:5000')

    expect(sent[0]?.url).toBe('/api/integrations/registries/registry.example.com%3A5000')
    expect(sent[0]?.method).toBe('DELETE')
  })

  it('sends a credential in the body of a PUT, and removes it with a DELETE', async () => {
    const sent = record()

    await setRegistryCredential('ghcr.io', { username: 'reader', token: 'not-a-real-token' })
    await removeRegistryCredential('ghcr.io')

    expect(sent).toEqual([
      {
        url: '/api/integrations/registries/ghcr.io/credential',
        method: 'PUT',
        body: { username: 'reader', token: 'not-a-real-token' },
      },
      { url: '/api/integrations/registries/ghcr.io/credential', method: 'DELETE', body: undefined },
    ])
  })

  it('keeps a repository path as its segments, under entry/', async () => {
    const sent = record()

    await addRegistryRepository('ghcr.io', 'fieldstatenz/saas-fabric')
    await removeRegistryRepository('docker.io', 'library/nginx')

    expect(sent.map(({ method, url }) => `${method} ${url}`)).toEqual([
      'PUT /api/integrations/registries/ghcr.io/repositories/entry/fieldstatenz/saas-fabric',
      'DELETE /api/integrations/registries/docker.io/repositories/entry/library/nginx',
    ])
  })

  it('encodes what is unusual inside a segment rather than letting it end the path', async () => {
    const sent = record()

    await addRegistryRepository('ghcr.io', 'acme/reports:1.0?x#y')

    expect(sent[0]?.url).toBe(
      '/api/integrations/registries/ghcr.io/repositories/entry/acme/reports%3A1.0%3Fx%23y',
    )
  })

  it('reads a repository’s versions from its own route', async () => {
    const sent = record(200, { tags: ['1.2.0', '1.1.0'], other: 3 })

    const versions = await registryVersions('ghcr.io', 'fieldstatenz/saas-fabric')

    expect(sent[0]?.url).toBe('/api/integrations/registries/ghcr.io/versions/fieldstatenz/saas-fabric')
    expect(versions).toEqual({ tags: ['1.2.0', '1.1.0'], other: 3 })
  })
})
