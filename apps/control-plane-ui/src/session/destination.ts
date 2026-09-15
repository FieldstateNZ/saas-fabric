/** Keep only an internal console hash through the external sign-in round trip. */
const DESTINATION = 'fabric.signin.destination'
export function rememberDestination(): void {
  if (window.location.hash.startsWith('#/')) {
    sessionStorage.setItem(DESTINATION, window.location.hash)
  }
}
export function consumeDestination(): string {
  const destination = sessionStorage.getItem(DESTINATION)
  sessionStorage.removeItem(DESTINATION)
  return window.location.hash.startsWith('#/') ? window.location.hash
    : destination?.startsWith('#/') ? destination : ''
}
