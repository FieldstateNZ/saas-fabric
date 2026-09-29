import { useEffect, useRef, useState } from 'react'

import {
  addRegistryRepository,
  listRegistries,
  registerRegistry,
  removeRegistry,
  removeRegistryCredential,
  removeRegistryRepository,
  setRegistryCredential,
} from '../api/registries'
import type {
  Registry,
  RegistryCredentialInput,
  RegistryListing,
  RegistryRegistration,
} from '../api/registry-types'
import { CHANGE_STILL_RUNNING, refusalOf, upsert, without } from './registry-listing'
import type { Apply, Refusal } from './registry-listing'

export type { Refusal } from './registry-listing'

/** How long after a change the control plane did not answer in time the list is read again. */
const STILL_RUNNING_REREAD_MS = 10_000

/**
 * The image registries, and every change an operator can make to them (ADR
 * 0026 section 5).
 *
 * Each change resolves to `null` once the control plane has proven and
 * recorded it, or to the refusal to show beside the form that asked.
 *
 * # What is shown is what the control plane holds
 *
 * A change that lands answers with the registry as it now stands, and that
 * view replaces the one shown -- nothing is inferred from what was asked. A
 * change that is refused is followed by a quiet read of the listing,
 * because a refusal can itself change what is true: a credential its realm
 * refused while a repository was being proven is marked refused, and the
 * section must say so without a reload. A quiet read that fails leaves the
 * last listing, which was true when it was read; one that a later change
 * overtook is dropped, so an older reading never replaces a newer answer.
 * A change the control plane did not answer in time may still be finishing,
 * so the list is read again once it has had time to.
 *
 * `busy` is one flag, not one per registry: the control plane takes every
 * registry change in one turn, so a second button pressed meanwhile would
 * only queue behind the first.
 */
export interface RegistriesState {
  readonly value: RegistryListing | null
  readonly loading: boolean
  readonly loadError: Refusal | null
  readonly busy: boolean
  readonly register: (registration: RegistryRegistration) => Promise<Refusal | null>
  readonly remove: (host: string) => Promise<Refusal | null>
  readonly setCredential: (
    host: string,
    credential: RegistryCredentialInput,
  ) => Promise<Refusal | null>
  readonly removeCredential: (host: string) => Promise<Refusal | null>
  readonly addRepository: (host: string, path: string) => Promise<Refusal | null>
  readonly removeRepository: (host: string, path: string) => Promise<Refusal | null>
}

export function useRegistries(): RegistriesState {
  const [value, setValue] = useState<RegistryListing | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<Refusal | null>(null)
  const [busy, setBusy] = useState(false)
  // Bumped by every change applied, so a quiet read started before it is
  // known to be older than what is shown.
  const generation = useRef(0)
  const mounted = useRef(true)
  const later = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    let current = true
    mounted.current = true

    listRegistries().then(
      (listing) => {
        if (current) {
          setValue(listing)
          setLoading(false)
        }
      },
      (error: unknown) => {
        if (current) {
          setLoadError(refusalOf(error))
          setLoading(false)
        }
      },
    )

    return () => {
      current = false
      mounted.current = false
      if (later.current !== null) {
        clearTimeout(later.current)
      }
    }
  }, [])

  function reread() {
    if (!mounted.current) {
      return
    }
    const started = generation.current
    listRegistries().then(
      (listing) => {
        if (mounted.current && generation.current === started) {
          setValue(listing)
        }
      },
      () => undefined,
    )
  }

  async function run(change: () => Promise<Apply>): Promise<Refusal | null> {
    setBusy(true)

    try {
      const apply = await change()
      generation.current += 1
      setValue((listing) => (listing === null ? listing : apply(listing)))
      return null
    } catch (error: unknown) {
      const refusal = refusalOf(error)
      if (refusal.code === CHANGE_STILL_RUNNING) {
        later.current = setTimeout(reread, STILL_RUNNING_REREAD_MS)
      } else {
        reread()
      }
      return refusal
    } finally {
      setBusy(false)
    }
  }

  const replacing = async (answer: Promise<Registry>): Promise<Apply> => upsert(await answer)

  return {
    value,
    loading,
    loadError,
    busy,
    register: (registration) => run(() => replacing(registerRegistry(registration))),
    remove: (host) =>
      run(async () => {
        await removeRegistry(host)
        return without(host)
      }),
    setCredential: (host, credential) =>
      run(() => replacing(setRegistryCredential(host, credential))),
    removeCredential: (host) => run(() => replacing(removeRegistryCredential(host))),
    addRepository: (host, path) => run(() => replacing(addRegistryRepository(host, path))),
    removeRepository: (host, path) => run(() => replacing(removeRegistryRepository(host, path))),
  }
}
