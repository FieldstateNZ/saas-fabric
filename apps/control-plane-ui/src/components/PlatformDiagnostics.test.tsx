import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { InvalidReasonCode, PlatformDiagnostic } from '../api/types'
import { diagnosticWording, PlatformDiagnostics } from './PlatformDiagnostics'

/**
 * Every reason code, checked complete by the compiler: a code added to the
 * type and not here fails `satisfies`, so this list cannot quietly fall behind
 * the one the wording is keyed by.
 */
const EVERY_REASON = Object.keys({
  unreadable: true,
  unsupportedVersion: true,
  wrongVersion: true,
  several: true,
  primaryNotNamed: true,
  otherRegistry: true,
  missingImage: true,
  noSingleRevision: true,
  notRegistered: true,
  notPinned: true,
} satisfies Record<InvalidReasonCode, true>) as InvalidReasonCode[]

describe('the versions that were not selected', () => {
  it('words each state for itself', () => {
    render(
      <PlatformDiagnostics
        diagnostics={[
          { version: '0.3.0-preview.6', state: 'publishing' },
          { version: '0.3.0-preview.5', state: 'undescribed' },
          { version: '0.3.0-preview.4', state: 'incoherent' },
          { version: '0.3.0-preview.3', state: 'invalid', reason: 'several' },
        ]}
      />,
    )

    const rows = screen.getAllByRole('listitem').map((row) => row.textContent)

    expect(rows).toEqual([
      '0.3.0-preview.6 — still publishing',
      '0.3.0-preview.5 — no component descriptor attached',
      '0.3.0-preview.4 — built more than once',
      '0.3.0-preview.3 — component descriptor cannot be used: more than one is attached, or more are listed than Fabric checks',
    ])
  })

  it('never words a version with nothing attached as publishing or built twice', () => {
    // Fabric cannot tell a publication in progress from one that will never
    // attach a component descriptor, so it says only what it saw.
    render(
      <PlatformDiagnostics diagnostics={[{ version: '0.3.0-preview.3', state: 'undescribed' }]} />,
    )

    expect(screen.getByText(/no component descriptor attached/)).toBeInTheDocument()
    expect(screen.queryByText(/still publishing/)).not.toBeInTheDocument()
    expect(screen.queryByText(/built more than once/)).not.toBeInTheDocument()
  })

  it('gives every reason its own plain words after the state', () => {
    const wordings = EVERY_REASON.map((reason) =>
      diagnosticWording({ version: '0.3.0-preview.3', state: 'invalid', reason }),
    )

    for (const wording of wordings) {
      expect(wording).toMatch(/^component descriptor cannot be used: \S/)
      // Plain words, not the code the API sent.
      expect(EVERY_REASON.some((reason) => wording.includes(reason))).toBe(false)
    }
    // None shares another's words, so none has fallen through to another.
    expect(new Set(wordings).size).toBe(EVERY_REASON.length)
  })

  it('says a newer format is one this Fabric does not read, not that it is broken', () => {
    const diagnostic: PlatformDiagnostic = {
      version: '0.3.0-preview.3',
      state: 'invalid',
      reason: 'unsupportedVersion',
    }

    expect(diagnosticWording(diagnostic)).toBe(
      'component descriptor cannot be used: it is written in a format version this Fabric does not read',
    )
  })

  it('names the format version it found when the control plane sends it', () => {
    const diagnostic: PlatformDiagnostic = {
      version: '0.3.0-preview.3',
      state: 'invalid',
      reason: 'unsupportedVersion',
      found: 'v2',
    }

    expect(diagnosticWording(diagnostic)).toBe(
      'component descriptor cannot be used: it is written in format version v2, which this Fabric does not read',
    )
  })

  it('says a pin differs in what it compares, never that every newer image differs', () => {
    const wording = diagnosticWording({
      version: '0.3.0-preview.3',
      state: 'invalid',
      reason: 'notPinned',
    })

    expect(wording).toMatch(/roles, repositories or a primary image/)
  })

  it('renders nothing when every version was selected or none exist', () => {
    const { container } = render(<PlatformDiagnostics diagnostics={[]} />)

    expect(container).toBeEmptyDOMElement()
  })
})
