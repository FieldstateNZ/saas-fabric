/**
 * The words the registries section uses for what the control plane
 * reports, kept in one place so every card says the same thing the same
 * way.
 */
import type { RegistryKind } from '../../api/registry-types'

/** What each kind is called on screen: its product name, and the API's word for it. */
export const KIND_NAMES: Readonly<Record<RegistryKind, string>> = {
  ghcr: 'GitHub Container Registry (ghcr)',
  dockerHub: 'Docker Hub (dockerHub)',
  distribution: 'Distribution API (distribution)',
}

/** The kinds, in the order the register form offers them. */
export const KIND_OPTIONS: readonly { readonly value: RegistryKind; readonly label: string }[] = [
  { value: 'ghcr', label: 'GitHub Container Registry — ghcr.io' },
  { value: 'dockerHub', label: 'Docker Hub — docker.io' },
  { value: 'distribution', label: 'Another registry, at an HTTPS origin you give' },
]

/** A moment the control plane recorded, in Unix seconds, as this browser writes dates. */
export function when(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString()
}

/**
 * An example repository under `host`, for a hint.
 *
 * Docker Hub's official images live under `library/`, and a repository of
 * one segment there is refused -- so the hint shows the spelling that is
 * accepted rather than the one people type.
 */
export function exampleUnder(host: string): string {
  return host === 'docker.io' ? 'docker.io/library/nginx' : `${host}/acme/reports`
}
