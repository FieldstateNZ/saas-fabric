import { useRef, useState } from 'react'

import { createClient, getProduct, saveProduct } from '../api/catalogue'
import type { Catalogue, ClientProductRequest, ClientProductResponse } from '../api/catalogue-types'
import { isControlPlaneError } from '../api/errors'
import { clientHref } from '../console/navigation'
import { PageHeader } from '../console/PageHeader'
import { describe } from '../hooks/useClients'
import { Assignments } from './Assignments'
import { ClientDetailsStep } from './ClientDetailsStep'
import { ClientFormActions } from './ClientFormActions'
import { ClientReviewStep } from './ClientReviewStep'
import { initialClientProductRequest } from './initialClientProductRequest'
import { SaveNotice } from './SaveNotice'
import { WizardSteps } from './WizardSteps'

const STEPS = ['Client details', 'Applications', 'Review'] as const

/**
 * Creating a client, or reconfiguring an existing one, as a three-step
 * wizard: details, application assignments, then a review before it writes
 * anything.
 *
 * # One submit, and only one, per click
 *
 * `submit` is guarded by a `submitting` ref that blocks a second call while
 * one is already in flight, because React does not disable a button
 * synchronously with the click that should have triggered the disable —
 * see {@link ClientFormActions} for the other half of this: why the final
 * action is a plain button rather than form submission, and how it ignores
 * a double-click's second event.
 *
 * # A conflict here means the operator's edits were never applied
 *
 * `existing.client.revision` conditions the write, the same way
 * `putIdentity` in `api/client.ts` does. A `revision_conflict` most often
 * means an identity edit on this same client — which writes its own
 * activity entry into the same document — moved the revision after this
 * form opened. Retrying with the same stale revision would only be refused
 * again, so this form does not retry: it says the client changed, and reads
 * the fresh product for `onStale` to hand back to whatever opened this form,
 * rather than silently discarding the operator's edits without telling them.
 *
 * This file sits in file-size-policy.md's 121-150 line band. `submit` and
 * `reload` need the same `id`, `existing` and `value` the three steps
 * render from, so splitting them out would mean passing that same state
 * across a file boundary for no reason; the three steps and the footer
 * already are their own files (`ClientDetailsStep`, `ClientReviewStep`,
 * `ClientFormActions`).
 */
export function ClientForm({
  catalogue,
  existing,
  onSaved,
  onStale,
  onCancel,
}: {
  catalogue: Catalogue
  existing?: ClientProductResponse
  onSaved: () => void
  onStale?: (fresh: ClientProductResponse) => void
  onCancel?: () => void
}) {
  const [id, setId] = useState(existing?.client.id ?? '')
  const [value, setValue] = useState<ClientProductRequest>(() =>
    initialClientProductRequest(catalogue, existing),
  )
  const [hostText, setHostText] = useState(existing?.client.hosts.join(', ') ?? '')
  const [step, setStep] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [conflict, setConflict] = useState(false)
  const submitting = useRef(false)

  async function submit() {
    if (submitting.current) {
      return
    }

    submitting.current = true
    setBusy(true)
    setError(null)
    setConflict(false)

    try {
      if (existing) {
        await saveProduct(id, existing.client.revision, value)
      } else {
        await createClient(id, value)
      }
      onSaved()
      if (!existing) {
        window.location.hash = clientHref(id).slice(1)
      }
    } catch (thrown: unknown) {
      if (existing && isControlPlaneError(thrown) && thrown.isConflict) {
        setError('This client changed since this form was opened. Your edits here were not saved.')
        setConflict(true)
      } else {
        setError(describe(thrown))
      }
    } finally {
      submitting.current = false
      setBusy(false)
    }
  }

  async function reload() {
    if (!existing) {
      return
    }

    try {
      onStale?.(await getProduct(existing.client.id))
    } catch (thrown: unknown) {
      setError(describe(thrown))
    }
  }

  const reloadAfterConflict = conflict
    ? () => {
        void reload()
      }
    : undefined

  return (
    <>
      <PageHeader
        eyebrow="Clients"
        title={existing ? `Configure ${existing.client.displayName}` : 'Create client'}
        description="Set up the client, choose published applications, and review before saving."
      />
      <SaveNotice error={error} success={null} onReload={reloadAfterConflict} />
      <WizardSteps steps={STEPS} current={step} />
      <form
        onSubmit={(event) => {
          event.preventDefault()
          if (step < 2) {
            setStep(step + 1)
          }
        }}
      >
        <fieldset disabled={busy}>
          {step === 0 && (
            <ClientDetailsStep
              id={id}
              onIdChange={setId}
              existing={Boolean(existing)}
              value={value}
              onValueChange={setValue}
              hostText={hostText}
              onHostTextChange={setHostText}
              clientFields={catalogue.clientFields}
            />
          )}
          {step === 1 && (
            <Assignments
              apps={catalogue.applications}
              value={value.applications}
              locked={existing?.product.applications.map((a) => a.applicationId) ?? []}
              onChange={(applications) => {
                setValue({ ...value, applications })
              }}
            />
          )}
          {step === 2 && <ClientReviewStep id={id} value={value} />}
          <ClientFormActions
            step={step}
            busy={busy}
            existing={Boolean(existing)}
            onBack={() => {
              setStep(step - 1)
            }}
            onSubmitFinal={() => {
              void submit()
            }}
            onCancel={
              onCancel ??
              (() => {
                window.location.hash = '/clients'
              })
            }
          />
        </fieldset>
      </form>
    </>
  )
}
