/**
 * What the console asks about this deployment's own composition.
 *
 * Split from `client.ts` because it is a different conversation: those calls
 * are about the clients this platform manages, these are about the platform
 * itself. They share one origin and one `request`, and nothing else.
 */
import { request } from './client'
import type { Platform, PlatformIntegration, RollbackCandidates } from './types'
import type { DataSourceInput, DataSources } from './data-source-types'

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

/**
 * What this component could be rolled back to.
 *
 * The list is the only way to name a version. There is no field to type one
 * into, and the control plane refuses anything it has not itself resolved to a
 * complete coherent release unit.
 */
export async function rollbackCandidates(component: string): Promise<RollbackCandidates> {
  return request<RollbackCandidates>(
    `/api/platform/components/${encodeURIComponent(component)}/versions`,
  )
}

/**
 * Puts a component back on an older version, and holds it there.
 *
 * Sends a version and nothing else. The three image digests are resolved by
 * the platform at the moment of the write, so there is no shape in which a
 * browser could name one.
 */
export async function rollBackComponent(
  component: string,
  version: string,
  note: string | null,
): Promise<void> {
  await request(`/api/platform/components/${encodeURIComponent(component)}/rollback`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ version, note }),
  })
}

/**
 * Every data source declared for this deployment's one environment
 * (ADR 0023 part 1).
 *
 * Takes no environment name, for the same reason `getPlatform` does not: a
 * deployment manages the one environment it was deployed into, and there is
 * nowhere else for a second name to reach.
 */
export async function getDataSources(): Promise<DataSources> {
  return request<DataSources>('/api/platform/data-sources')
}

/**
 * Declares a data source, or corrects one that already exists.
 *
 * `revision` is the document revision this operator last read, always sent
 * as `If-Match`. There is no "nothing declared yet" case that skips it: the
 * late-bound binding always has a fact to compare-and-swap on -- the
 * generation tag, even when no file exists -- so the very first declaration
 * in an environment reads that tag from `getDataSources` and sends it back,
 * exactly like every later one. `If-None-Match` is not accepted on this
 * route.
 */
export async function declareDataSource(
  id: string,
  input: DataSourceInput,
  revision: string,
): Promise<DataSources> {
  return request<DataSources>(`/api/platform/data-sources/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
      'If-Match': `"${revision}"`,
    },
    body: JSON.stringify(input),
  })
}

/**
 * Removes a data source that no placement references.
 *
 * `revision` is the data-sources document revision this operator last read,
 * always sent as `If-Match` -- the same precondition `declareDataSource`
 * sends. A data source a held placement still names is refused as
 * `409 data_source_in_use`, with the tenants that hold it in the message;
 * removing one nothing references answers with the data-sources list, the
 * same body `getDataSources` and `declareDataSource` return.
 */
export async function removeDataSource(id: string, revision: string): Promise<DataSources> {
  return request<DataSources>(`/api/platform/data-sources/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    headers: { 'If-Match': `"${revision}"` },
  })
}
