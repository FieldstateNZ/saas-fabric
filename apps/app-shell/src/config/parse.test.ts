import { describe, expect, it } from 'vitest'
import { parseConfig } from './parse'
import example from './app.yaml?raw'

const minimal = 'branding: {name: Example}\nnavigation: [{id: home, label: Home, content: welcome}]'

describe('configuration validation', () => {
  it('accepts the example and supplies sensible defaults for a minimal configuration', () => {
    expect(parseConfig(example).ok).toBe(true)
    const result = parseConfig(minimal)
    expect(result.ok && result.config.layout.sidebar.collapsible).toBe(true)
    expect(result.ok && result.config.theme.primary).toBe('#4a5c42')
  })
  it.each([
    ['malformed YAML', 'branding: ['],
    ['duplicate YAML keys', `${minimal}\nbranding: {name: Duplicate}`],
    ['unknown property', `${minimal}\nlayout: {hedaer: [brand]}`],
    ['unknown content', minimal.replace('welcome', 'run-script')],
    ['duplicate navigation ID', minimal.replace('content: welcome}', 'content: welcome}, {id: home, label: Other, content: notes}')],
    ['empty navigation', 'branding: {name: Example}\nnavigation: []'],
    ['invalid color', `${minimal}\ntheme: {primary: red}`],
    ['alias expansion', 'branding: &brand {name: Example}\ntheme: *brand'],
    ['unknown tag', 'branding: !script {name: Example}'],
    ['missing navigation region', `${minimal}\nlayout: {header: [brand], sidebar: {regions: []}}`],
    ['duplicate navigation region', `${minimal}\nlayout: {header: [navigation]}`],
    ['missing page region', `${minimal}\nlayout: {content: [configuration]}`],
  ])('rejects %s with readable errors', (_name, source) => {
    const result = parseConfig(source)
    expect(result.ok).toBe(false)
    expect(!result.ok && result.errors.length).toBeGreaterThan(0)
  })
  it('identifies the invalid field', () => {
    const result = parseConfig(`${minimal}\ntheme: {primary: red}`)
    expect(!result.ok && result.errors[0]).toContain('theme.primary')
  })
})
