/**
 * What the console asks about image registries (ADR 0026 section 5).
 *
 * Split from `client.ts` the way `placements.ts` is, sharing the one
 * `request` and the one origin every call in this directory goes through.
 * The console never reaches a registry itself: every proof, and every tag
 * listing, is the control plane's.
 *
 * # Fixed functions, and a host that is only a key
 *
 * One function per route, as `IntegrationEndpoints` has, and no kind
 * threaded through: which integration a call acts on is decided by the
 * route, never by an argument. A registry's host is a key into records the
 * control plane holds -- this builds a path segment from it, never a
 * location to fetch.
 */
import { request } from './client'
import type {
  ComponentReads,
  Registry,
  RegistryCredentialInput,
  RegistryListing,
  RegistryRegistration,
  RegistryVersions,
} from './registry-types'

const REGISTRIES = '/api/integrations/registries'

const JSON_BODY = { 'Content-Type': 'application/json' }

/** One registry's routes. */
function registry(host: string): string {
  return `${REGISTRIES}/${encodeURIComponent(host)}`
}

/**
 * A repository's path, encoded per segment so it keeps its structure.
 *
 * `underHost` has already refused an empty, `.` or `..` segment -- which a
 * browser would resolve away before the request left, sending it to another
 * route altogether. The control plane validates the path again on arrival.
 */
function segments(path: string): string {
  return path.split('/').map(encodeURIComponent).join('/')
}

/**
 * Where a repository written in full goes under its registry's routes:
 * everything after `{host}/`, or why it cannot go there.
 *
 * An operator types a repository whole, as every image reference names it.
 * One that does not start with this registry's host is not this registry's,
 * and is refused here rather than sent under a host it does not name.
 */
export function underHost(
  host: string,
  repository: string,
): { readonly path: string } | { readonly problem: string } {
  const prefix = `${host}/`

  if (!repository.startsWith(prefix) || repository.length === prefix.length) {
    return { problem: `A repository here is written in full, starting with ${prefix}` }
  }

  const path = repository.slice(prefix.length)

  if (path.split('/').some((segment) => segment === '' || segment === '.' || segment === '..')) {
    return { problem: 'A repository has no empty, "." or ".." segment.' }
  }

  return { path }
}

/** Every registry operators registered, and the deployment's own. */
export async function listRegistries(): Promise<RegistryListing> {
  return request<RegistryListing>(REGISTRIES)
}

/** Which registry each managed component's images are read through, and how. */
export async function registryReads(): Promise<ComponentReads> {
  return request<ComponentReads>(`${REGISTRIES}/reads`)
}

/** Registers a registry, once its `/v2/` endpoint has proven. */
export async function registerRegistry(registration: RegistryRegistration): Promise<Registry> {
  return request<Registry>(REGISTRIES, {
    method: 'POST',
    headers: JSON_BODY,
    body: JSON.stringify(registration),
  })
}

/** Removes a registry, its credential and its repositories. */
export async function removeRegistry(host: string): Promise<void> {
  await request<undefined>(registry(host), { method: 'DELETE' })
}

/**
 * Sets or replaces a registry's credential, once the registry and every
 * repository registered under it prove with it.
 */
export async function setRegistryCredential(
  host: string,
  credential: RegistryCredentialInput,
): Promise<Registry> {
  return request<Registry>(`${registry(host)}/credential`, {
    method: 'PUT',
    headers: JSON_BODY,
    body: JSON.stringify(credential),
  })
}

/** Removes a registry's credential; it is read anonymously from then on. */
export async function removeRegistryCredential(host: string): Promise<Registry> {
  return request<Registry>(`${registry(host)}/credential`, { method: 'DELETE' })
}

/** Registers `{host}/{path}`, once its tag listing answers. */
export async function addRegistryRepository(host: string, path: string): Promise<Registry> {
  return request<Registry>(`${registry(host)}/repositories/entry/${segments(path)}`, {
    method: 'PUT',
  })
}

/** Removes `{host}/{path}` from its registry. */
export async function removeRegistryRepository(host: string, path: string): Promise<Registry> {
  return request<Registry>(`${registry(host)}/repositories/entry/${segments(path)}`, {
    method: 'DELETE',
  })
}

/** `{host}/{path}`'s version tags, newest first, for the picker. */
export async function registryVersions(host: string, path: string): Promise<RegistryVersions> {
  return request<RegistryVersions>(`${registry(host)}/versions/${segments(path)}`)
}
