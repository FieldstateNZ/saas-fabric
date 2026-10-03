/**
 * The console's union of authored and declared content, pinned against the
 * same fixture as `fabric-client-model`'s `catalogue/effective_tests.rs`, so
 * a client's configuration form asks for exactly what the server requires.
 */
import { describe, expect, it } from 'vitest'

import type { ConfigurationField } from '../api/catalogue-types'
import {
  authored,
  definition,
  described,
  FIRST,
  resolution,
  SECOND,
} from './component/described.fixture'
import { declaring, effectiveFields, effectiveResources } from './effective'

/** An optional text field `key`. */
function field(key: string): ConfigurationField {
  return { key, label: key, kind: 'text', required: false, default: null, options: [], description: '' }
}

describe('effectiveFields and effectiveResources', () => {
  it('are authored then declared in component order, and leave the authored lists alone', () => {
    const base = definition([
      authored('web', 'container'),
      described('reports', resolution('1.4.0', FIRST)),
    ])
    const withRegion = { ...base, fields: [field('region')] }

    expect(effectiveFields(withRegion).map((field) => field.key)).toEqual(['region', 'team'])
    expect(effectiveResources(withRegion).map((resource) => resource.name)).toEqual(['reports'])
    expect(withRegion.fields).toHaveLength(1)
    expect(withRegion.resources).toBeUndefined()
  })

  it('follow component order across several described components', () => {
    const audit = resolution('2.0.0', SECOND, [field('audit')], [])
    const both = definition([
      described('audit', audit),
      described('reports', resolution('1.4.0', FIRST)),
    ])

    expect(effectiveFields(both).map((field) => field.key)).toEqual(['audit', 'team'])
    expect(declaring(both).map(({ component }) => component.id)).toEqual(['audit', 'reports'])
  })

  it('count nothing for a described component still waiting for its version', () => {
    const pending = {
      ...authored('reports', 'container'),
      kind: 'described' as const,
      reference: '',
      version: '',
    }

    expect(effectiveFields(definition([pending]))).toEqual([])
    expect(effectiveResources(definition([pending]))).toEqual([])
  })

  it('treat a definition the server sent without a resources key as authoring none', () => {
    expect(effectiveResources(definition([]))).toEqual([])
  })
})
