import { useState } from 'react'

import { createClient, saveProduct } from '../api/catalogue'
import type { Catalogue, ClientProductRequest, ClientProductResponse } from '../api/catalogue-types'
import { clientHref } from '../console/navigation'
import { PageHeader } from '../console/PageHeader'
import { describe } from '../hooks/useClients'
import { Assignments } from './Assignments'
import { ClientDetailsStep } from './ClientDetailsStep'
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
 * One `submit` for both creation and reconfiguration, branching on whether
 * `existing` was passed in — the request shape (`ClientProductRequest`) is
 * the same whole-replacement either way, and `existing.client.revision` is
 * what conditions the write so a concurrent edit is refused rather than
 * silently overwritten, the same way `putIdentity` in `api/client.ts` is.
 */
export function ClientForm({
  catalogue,
  existing,
  onSaved,
  onCancel,
}: {
  catalogue: Catalogue
  existing?: ClientProductResponse
  onSaved: () => void
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

  async function submit() {
    setBusy(true)
    setError(null)
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
    } catch (error: unknown) {
      setError(describe(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <PageHeader
        eyebrow="Clients"
        title={existing ? `Configure ${existing.client.displayName}` : 'Create client'}
        description="Set up the client, choose published applications, and review before saving."
      />
      <SaveNotice error={error} success={null} />
      <WizardSteps steps={STEPS} current={step} />
      <form
        onSubmit={(event) => {
          event.preventDefault()
          if (step < 2) {
            setStep(step + 1)
          } else {
            void submit()
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
          <div className="form-actions">
            {step > 0 && (
              <button
                type="button"
                onClick={() => {
                  setStep(step - 1)
                }}
              >
                Back
              </button>
            )}
            <button type="submit" className="primary-button">
              {busy
                ? 'Saving…'
                : step < 2
                  ? 'Continue'
                  : existing
                    ? 'Save client configuration'
                    : 'Create client'}
            </button>
            <button
              type="button"
              onClick={() => {
                if (onCancel) {
                  onCancel()
                } else {
                  window.location.hash = '/clients'
                }
              }}
            >
              Cancel
            </button>
          </div>
        </fieldset>
      </form>
    </>
  )
}
