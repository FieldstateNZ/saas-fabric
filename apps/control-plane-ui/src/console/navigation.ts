import { useEffect, useState } from 'react'

/**
 * Every page the sidebar links to, in the order it lists them.
 *
 * A tuple of key and label, not a `Record`, because {@link Sidebar} needs the
 * order preserved and a plain object would not promise that. `create-client`
 * is deliberately absent — it is reachable, via {@link Page}, but it is a
 * flow the sidebar does not link to directly.
 */
export const NAVIGATION = [
  ['overview', 'Overview'],
  ['clients', 'Clients'],
  ['applications', 'Applications'],
  ['components', 'Components'],
  ['definition', 'Client definition'],
  ['environments', 'Environments'],
  ['integrations', 'Integrations'],
  ['reconciliation', 'Reconciliation'],
  ['settings', 'Settings'],
] as const

/** Every page the console can be routed to. */
export type Page = (typeof NAVIGATION)[number][0] | 'create-client'

/** Where the console is: which page, and which client or application it is showing, if any. */
export interface Route {
  readonly page: Page
  readonly clientId: string | null
  readonly applicationId?: string | null
}

/**
 * Reads the current route from the URL.
 *
 * # The Git host's callback wins first
 *
 * `session.ts` sends the browser away for sign-in, but a Git host's
 * connection callback (`?git=...`, `?platform=...`) is a normal top-level
 * navigation the console itself asked for — see `IntegrationEndpoints`. That
 * callback lands on whatever hash route happened to be current, which is
 * rarely Integrations, so a query key from either flow is treated as an
 * instruction to land there regardless of the hash.
 *
 * # Hash URLs survive reloads
 *
 * The production server serves the root document without a history
 * fallback, so a route encoded any other way would 404 on refresh. A
 * malformed hash — a percent-escape `decodeURIComponent` cannot parse —
 * falls back to the client list rather than throwing, because a bad deep
 * link is not a reason to crash the console that received it.
 */
export function readRoute(): Route {
  const query = new URLSearchParams(window.location.search)
  if (['git', 'git_error', 'platform', 'platform_error'].some((key) => query.has(key))) {
    return { page: 'integrations', clientId: null }
  }

  const [page, id] = window.location.hash.slice(2).split('/')
  const found = NAVIGATION.find(([key]) => key === page)

  let clientId: string | null = null
  try {
    clientId = id ? decodeURIComponent(id) : null
  } catch {
    /* Malformed links fall back to the list. */
  }

  return {
    page: page === 'create-client' ? 'create-client' : (found?.[0] ?? 'overview'),
    clientId: page === 'clients' ? clientId : null,
    ...(page === 'applications' ? { applicationId: clientId } : {}),
  }
}

/** The hash route for one client's detail page. */
export function clientHref(id: string): string {
  return `#/clients/${encodeURIComponent(id)}`
}

/** The current route, kept live as the operator navigates. */
export function useRoute(): Route {
  const [route, setRoute] = useState(readRoute)

  useEffect(() => {
    const update = () => {
      setRoute(readRoute())
    }
    window.addEventListener('hashchange', update)
    return () => {
      window.removeEventListener('hashchange', update)
    }
  }, [])

  return route
}
