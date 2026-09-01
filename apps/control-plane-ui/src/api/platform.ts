/**
 * What the console asks about this deployment's own composition.
 *
 * Split from `client.ts` because it is a different conversation: those calls
 * are about the clients this platform manages, these are about the platform
 * itself. They share one origin and one `request`, and nothing else.
 */
import { request } from './client'
import type { Platform, PlatformIntegration } from './types'

/**
 * What this deployment's environment is asked to run.
 *
 * Takes no environment name. A deployment manages the one it was deployed
 * into; a name in the URL would reach the platform repository as a path
 * segment, and the console has no business choosing one.
 *
 * Reading this cannot change anything. What advances an environment is the
 * control plane's own sweep, on the cadence its deployment configures — so a
 * refresh, a second operator, or a browser prefetching this page cannot move
 * a version.
 */
export async function getPlatform(): Promise<Platform> {
  return request<Platform>('/api/platform')
}

/**
 * Stops a component advancing, leaving the version it runs alone.
 *
 * The component is named; the environment is not. A component name is a key
 * looked up in a manifest the platform already read and trusts, and the control
 * plane refuses one the manifest does not carry — so naming it selects an entry
 * and reaches nothing else.
 */
export async function pauseComponent(component: string, note: string | null): Promise<void> {
  await request(`/api/platform/components/${encodeURIComponent(component)}/hold`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ note }),
  })
}

/**
 * Lets a component advance again.
 *
 * Lifts the hold and nothing else. What happens next is the next sweep's to
 * decide, so this reports no version and the console does not pretend one
 * moved.
 */
export async function resumeComponent(component: string): Promise<void> {
  await request(`/api/platform/components/${encodeURIComponent(component)}/hold`, {
    method: 'DELETE',
  })
}

/** The Platform Management application's lifecycle. */
export async function getPlatformIntegration(): Promise<PlatformIntegration> {
  return request<PlatformIntegration>('/api/integrations/platform')
}
