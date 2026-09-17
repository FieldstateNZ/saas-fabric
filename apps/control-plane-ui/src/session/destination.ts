/**
 * Where to send the browser back to after the identity provider's redirect.
 *
 * `session.ts` sends the browser away entirely for sign-in, so whatever hash
 * route the operator was on is gone the moment that navigation starts. This
 * remembers it across the round trip, and only ever hands back an internal
 * console route — never a value that could send the browser somewhere else.
 */
const DESTINATION = 'fabric.signin.destination'

/** Stashes the current hash route, if there is one worth returning to. */
export function rememberDestination(): void {
  if (window.location.hash.startsWith('#/')) {
    sessionStorage.setItem(DESTINATION, window.location.hash)
  }
}

/**
 * Returns the hash route to restore, and forgets it either way.
 *
 * Consumed once: a stale destination offered to a later, unrelated callback
 * would land the operator somewhere they never asked to go. The current hash
 * wins if one is already present — that happens when the browser still holds
 * the operator's last route and only the query string changed — and anything
 * stored that does not start with `#/` is refused rather than returned,
 * because that is the one shape `rememberDestination` never wrote.
 */
export function consumeDestination(): string {
  const destination = sessionStorage.getItem(DESTINATION)
  sessionStorage.removeItem(DESTINATION)

  if (window.location.hash.startsWith('#/')) {
    return window.location.hash
  }

  return destination?.startsWith('#/') ? destination : ''
}
