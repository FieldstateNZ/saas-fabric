/**
 * What a save sends, and how the local draft is compared (ADR 0026 section
 * 7): the request strips what the server resolved, and only from described
 * components; the comparisons keep it.
 */
import { describe, expect, it } from 'vitest'

import { authored, definition, described, FIRST, resolution } from './component/described.fixture'
import { keepingPending, sameDefinition, toDraftRequest, unsavedBesidesPending } from './draft-request'

const reports = described('reports', resolution('1.4.0', FIRST))

/** A described component added here, still waiting for a version. */
const waiting = { ...authored('audit', 'container'), kind: 'described' as const }

describe('toDraftRequest', () => {
  it('strips resolution, reference and version from described components only', () => {
    const request = toDraftRequest(
      definition([reports, authored('web', 'container'), authored('idp', 'capability')]),
    )

    expect(request.components).toEqual([
      { kind: 'described', id: 'reports', name: 'Reports', required: false, policy: 'manual' },
      {
        kind: 'container',
        id: 'web',
        name: 'Authored',
        reference: 'registry.example.com/web',
        version: '1.0.0',
        required: true,
        policy: 'automatic',
      },
      {
        kind: 'capability',
        id: 'idp',
        name: 'Authored',
        reference: 'Identity',
        version: '',
        required: true,
        policy: 'automatic',
      },
    ])
    expect(JSON.stringify(request)).not.toContain('sha256:')
    expect(JSON.stringify(request)).not.toContain('resolution')
  })

  it('leaves out a component still waiting for its version, which the server cannot hold', () => {
    const request = toDraftRequest(definition([reports, waiting]))

    expect(request.components.map((component) => component.id)).toEqual(['reports'])
  })

  it('keeps the local draft whole: the resolution is still there to compare', () => {
    const draft = definition([reports])
    toDraftRequest(draft)

    expect(draft.components[0]?.resolution?.descriptorDigest).toBe(FIRST)
  })
})

describe('sameDefinition and unsavedBesidesPending', () => {
  it('reads an absent resources key and an empty list as the same statement', () => {
    const absent = definition([reports])

    expect(sameDefinition(absent, { ...absent, resources: [] })).toBe(true)
  })

  it('compares resolutions like with like', () => {
    const released = definition([reports])
    const reselected = definition([described('reports', resolution('1.5.0', FIRST))])

    expect(sameDefinition(released, definition([reports]))).toBe(true)
    expect(sameDefinition(released, reselected)).toBe(false)
  })

  it('does not count a component still waiting for its version as an unsaved change', () => {
    const saved = definition([reports])
    expect(unsavedBesidesPending({ ...saved, components: [reports, waiting] }, saved)).toBe(false)
    expect(unsavedBesidesPending({ ...saved, name: 'Renamed' }, saved)).toBe(true)
  })
})

describe('keepingPending', () => {
  it('keeps the components still waiting for a version across what the server answered', () => {
    const answered = { ...definition([reports]), name: 'Renamed' }

    const kept = keepingPending(answered, definition([reports, waiting, authored('web', 'container')]))

    expect(kept.name).toBe('Renamed')
    expect(kept.components.map((component) => component.id)).toEqual(['reports', 'audit'])
  })

  it('drops a waiting component once the server holds one under its id', () => {
    const selected = described('audit', resolution('1.4.0', FIRST))

    const kept = keepingPending(definition([selected]), definition([waiting]))

    expect(kept.components).toEqual([selected])
  })
})
