import { afterEach, describe, expect, it } from 'vitest'
import { consumeDestination, rememberDestination } from './destination'
afterEach(() => { sessionStorage.clear(); window.history.replaceState({}, '', '/') })
describe('sign-in destinations', () => {
  it('restores an internal client URL after a callback and consumes it once', () => {
    window.history.replaceState({}, '', '/#/clients/acme')
    rememberDestination()
    window.history.replaceState({}, '', '/?code=callback')
    expect(consumeDestination()).toBe('#/clients/acme')
    expect(consumeDestination()).toBe('')
  })
  it('refuses an external stored destination', () => {
    sessionStorage.setItem('fabric.signin.destination', 'https://example.test')
    expect(consumeDestination()).toBe('')
  })
})
