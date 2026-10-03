import { isControlPlaneError } from '../api/errors'
import type { Registry, RegistryListing } from '../api/registry-types'
import { describe } from './useClients'

/**
 * A refusal as the registries section shows it: the control plane's code,
 * which decides how the message is introduced, and its message, shown as
 * it came.
 */
export interface Refusal {
  readonly code: string
  readonly message: string
}

/** How a change's answer alters the listing shown. */
export type Apply = (listing: RegistryListing) => RegistryListing

/**
 * The code a registry change is shown with when the control plane did not
 * answer before its request timed out.
 *
 * The console's own, never the control plane's: a `504` carries no body.
 * The control plane runs every registry change to the end in a task of its
 * own, so a request cut off by its timeout may still be finishing the
 * change -- which is what the operator is told, rather than a bare status
 * that reads as "it failed".
 */
export const CHANGE_STILL_RUNNING = 'registry_change_still_running'

/** The listing with `registry` in its place, or after the rest if it is new. */
export function upsert(registry: Registry): Apply {
  return (listing) => {
    const known = listing.registries.some((held) => held.host === registry.host)
    const registries = known
      ? listing.registries.map((held) => (held.host === registry.host ? registry : held))
      : [...listing.registries, registry]

    return { ...listing, registries }
  }
}

/** The listing without `host`. */
export function without(host: string): Apply {
  return (listing) => ({
    ...listing,
    registries: listing.registries.filter((held) => held.host !== host),
  })
}

/** Anything thrown, as a refusal to show. */
export function refusalOf(error: unknown): Refusal {
  if (isControlPlaneError(error) && error.status === 504) {
    return {
      code: CHANGE_STILL_RUNNING,
      message:
        'The control plane did not answer in time, and may still be finishing this change. The list will be read again shortly.',
    }
  }
  return isControlPlaneError(error)
    ? { code: error.code, message: error.message }
    : { code: 'unexpected', message: describe(error) }
}
