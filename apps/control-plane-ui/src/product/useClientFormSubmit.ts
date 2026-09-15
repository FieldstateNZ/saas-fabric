import { useRef, useState } from 'react'

import { createClient, getProduct, saveProduct } from '../api/catalogue'
import type { ClientProductRequest, ClientProductResponse } from '../api/catalogue-types'
import { isControlPlaneError } from '../api/errors'
import { clientHref } from '../console/navigation'
import { describe } from '../hooks/useClients'

/**
 * `ClientForm`'s write: create or save, and what each of its three refusals
 * means.
 *
 * # One submit, and only one, in flight
 *
 * `submitting` makes a second call to `submit` a no-op while one is already
 * running, whatever triggered it — see `ClientFormActions` for the defences
 * against a click or keypress ever reaching a second call in the first
 * place; this ref is the backstop for whatever gets past those.
 *
 * # A conflict means the operator's edits were never applied
 *
 * `existing.client.revision` conditions the write, the same way
 * `putIdentity` in `api/client.ts` does. `ClientForm` replaces the whole
 * client workspace while it is open, so a `revision_conflict` cannot be the
 * operator's own identity edit; it means another operator, or the same
 * operator in another tab, changed this client while the form was open.
 * Retrying with the same stale revision would only be refused again, so
 * this does not retry: `reload` reads the fresh product for `onStale` to
 * hand back to whatever opened the form.
 *
 * # A taken ID is reported on the field, not the form
 *
 * `client_exists` is the one refusal this can act on directly: it names
 * exactly what is wrong. `onIdTaken` tells `ClientForm` to return to step 0,
 * and `idError` carries the message for the Client ID field itself.
 */
export function useClientFormSubmit(options: {
  readonly id: string
  readonly existing: ClientProductResponse | undefined
  readonly value: ClientProductRequest
  readonly onSaved: () => void
  readonly onStale?: ((fresh: ClientProductResponse) => void) | undefined
  readonly onIdTaken: () => void
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [conflict, setConflict] = useState(false)
  const [idError, setIdError] = useState<string | null>(null)
  const submitting = useRef(false)

  async function submit() {
    if (submitting.current) {
      return
    }

    submitting.current = true
    setBusy(true)
    setError(null)
    setConflict(false)
    setIdError(null)

    try {
      if (options.existing) {
        await saveProduct(options.id, options.existing.client.revision, options.value)
      } else {
        await createClient(options.id, options.value)
      }
      options.onSaved()
      if (!options.existing) {
        window.location.hash = clientHref(options.id).slice(1)
      }
    } catch (thrown: unknown) {
      if (options.existing && isControlPlaneError(thrown) && thrown.isConflict) {
        setError('This client changed since this form was opened. Your edits here were not saved.')
        setConflict(true)
      } else if (!options.existing && isControlPlaneError(thrown) && thrown.code === 'client_exists') {
        options.onIdTaken()
        setIdError(thrown.message)
      } else {
        setError(describe(thrown))
      }
    } finally {
      submitting.current = false
      setBusy(false)
    }
  }

  async function reload() {
    if (!options.existing) {
      return
    }

    try {
      options.onStale?.(await getProduct(options.existing.client.id))
    } catch (thrown: unknown) {
      setError(describe(thrown))
    }
  }

  return {
    busy,
    error,
    idError,
    clearIdError: () => {
      setIdError(null)
    },
    reloadAfterConflict: conflict
      ? () => {
          void reload()
        }
      : undefined,
    submit: () => {
      void submit()
    },
  }
}
