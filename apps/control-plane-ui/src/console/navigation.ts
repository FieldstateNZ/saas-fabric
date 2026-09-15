import { useEffect, useState } from 'react'

export const NAVIGATION = [
  ['overview', 'Overview'], ['clients', 'Clients'], ['applications', 'Applications'],
  ['components', 'Components'], ['definition', 'Client definition'],
  ['environments', 'Environments'], ['integrations', 'Integrations'],
  ['reconciliation', 'Reconciliation'], ['settings', 'Settings'],
] as const
export type Page = (typeof NAVIGATION)[number][0] | 'create-client'
export interface Route { page: Page; clientId: string | null; applicationId?: string | null }

/** Hash URLs survive reloads on the production server and preserve OIDC callbacks. */
export function readRoute(): Route {
  const query = new URLSearchParams(window.location.search)
  if (['git', 'git_error', 'platform', 'platform_error'].some((key) => query.has(key))) {
    return { page: 'integrations', clientId: null }
  }
  const [page, id] = window.location.hash.slice(2).split('/')
  const found = NAVIGATION.find(([key]) => key === page)
  let clientId: string | null = null
  try { clientId = id ? decodeURIComponent(id) : null } catch { /* Malformed links fall back to the list. */ }
  return { page: page === 'create-client' ? 'create-client' : found?.[0] ?? 'overview', clientId: page === 'clients' ? clientId : null, ...(page === 'applications' ? { applicationId: clientId } : {}) }
}
export function clientHref(id: string): string { return `#/clients/${encodeURIComponent(id)}` }
export function useRoute(): Route {
  const [route, setRoute] = useState(readRoute)
  useEffect(() => {
    const update = () => { setRoute(readRoute()) }
    window.addEventListener('hashchange', update)
    return () => { window.removeEventListener('hashchange', update) }
  }, [])
  return route
}
