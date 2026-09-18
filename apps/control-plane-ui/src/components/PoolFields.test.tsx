import { describe, expect, it } from 'vitest'

import { hasInvalidPoolFields, poolFieldError } from './PoolFields'

describe('poolFieldError', () => {
  it('accepts a plain positive integer', () => {
    expect(poolFieldError('20')).toBeNull()
  })

  it('refuses an empty field rather than letting it become 0', () => {
    expect(poolFieldError('')).toBe('Enter a number.')
  })

  it('refuses a field that is only whitespace', () => {
    expect(poolFieldError('   ')).toBe('Enter a number.')
  })

  it('refuses text that Number() would turn into NaN', () => {
    expect(poolFieldError('abc')).toBe('Enter a number.')
  })
})

describe('hasInvalidPoolFields', () => {
  const valid = { maxConnections: '20', idleTimeoutSeconds: '300', acquireTimeoutSeconds: '5' }

  it('is false when all three fields are numbers', () => {
    expect(hasInvalidPoolFields(valid)).toBe(false)
  })

  it('is true when any one of the three is empty', () => {
    expect(hasInvalidPoolFields({ ...valid, idleTimeoutSeconds: '' })).toBe(true)
  })

  it('is true when any one of the three is non-numeric', () => {
    expect(hasInvalidPoolFields({ ...valid, acquireTimeoutSeconds: 'soon' })).toBe(true)
  })
})
