/**
 * A refusal's body, read into a `ControlPlaneError`: a refused selection
 * keeps the rule's answer and reason beside its code, and nothing else
 * invents one.
 */
import { describe, expect, it } from 'vitest'

import { refusalFromBody } from './errors'

describe('refusalFromBody', () => {
  it('keeps a refused selection’s answer and reason', () => {
    const error = refusalFromBody(422, {
      error: {
        code: 'component_version_unusable',
        message: 'cannot be selected',
        answer: 'invalid',
        reason: 'wrongVersion',
      },
    })

    expect(error?.code).toBe('component_version_unusable')
    expect(error?.answer).toEqual({ answer: 'invalid', reason: 'wrongVersion' })
  })

  it('carries no answer for an answer it does not know, or for any other refusal', () => {
    const unknown = refusalFromBody(422, {
      error: { code: 'component_version_unusable', message: 'm', answer: 'shrugged' },
    })
    const other = refusalFromBody(409, { error: { code: 'revision_conflict', message: 'stale' } })

    expect(unknown?.answer).toBeNull()
    expect(other?.answer).toBeNull()
    expect(other?.isConflict).toBe(true)
  })

  it('is nothing for a body that is not an API error', () => {
    expect(refusalFromBody(502, null)).toBeNull()
    expect(refusalFromBody(502, '<html>')).toBeNull()
    expect(refusalFromBody(502, { error: { code: 'x' } })).toBeNull()
  })
})
