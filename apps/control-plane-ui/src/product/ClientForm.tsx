import { useState } from 'react'

import type { Catalogue, ClientProductRequest, ClientProductResponse } from '../api/catalogue-types'
import { PageHeader } from '../console/PageHeader'
import { Assignments } from './Assignments'
import { ClientDetailsStep } from './ClientDetailsStep'
import { ClientFormActions } from './ClientFormActions'
import { ClientReviewStep } from './ClientReviewStep'
import { initialClientProductRequest } from './initialClientProductRequest'
import { SaveNotice } from './SaveNotice'
import { useClientFormSubmit } from './useClientFormSubmit'
import { WizardSteps } from './WizardSteps'

const STEPS = ['Client details', 'Applications', 'Review'] as const

/**
 * Creating a client, or reconfiguring an existing one, as a three-step
 * wizard: details, application assignments, then a review before it writes
 * anything.
 *
 * The write itself — `submit`, its `busy`/`error`/`idError` state, and what
 * each of the three refusals it can receive means — is `useClientFormSubmit`,
 * not this component: see that hook's own doc for a conflict, a taken ID,
 * and the in-flight guard. This component owns only the wizard's own state
 * (which step, and the three steps' draft values) and wires the hook's
 * outcomes to it — `onIdTaken` sends the operator back to step 0, the same
 * way `ClientWorkspace`'s `useConfigureClient` keeps its read separate from
 * that component's render tree.
 *
 * This file sits in file-size-policy.md's 121-150 line band: three steps to
 * hold state for, each with its own file already (`ClientDetailsStep`,
 * `ClientReviewStep`, `ClientFormActions`), is one wizard, not several
 * components sharing a lifecycle.
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

  const form = useClientFormSubmit({
    id,
    existing,
    value,
    onSaved,
    onStale,
    onIdTaken: () => {
      setStep(0)
    },
  })

  return (
    <>
      <PageHeader
        eyebrow="Clients"
        title={existing ? `Configure ${existing.client.displayName}` : 'Create client'}
        description="Set up the client, choose published applications, and review before saving."
      />
      <SaveNotice error={form.error} success={null} onReload={form.reloadAfterConflict} />
      <WizardSteps steps={STEPS} current={step} />
      <form
        onSubmit={(event) => {
          event.preventDefault()
          if (step < 2) {
            setStep(step + 1)
          }
        }}
      >
        <fieldset disabled={form.busy}>
          {step === 0 && (
            <ClientDetailsStep
              id={id}
              onIdChange={(next) => {
                setId(next)
                form.clearIdError()
              }}
              idError={form.idError}
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
            busy={form.busy}
            existing={Boolean(existing)}
            onBack={() => {
              setStep(step - 1)
            }}
            onSubmitFinal={form.submit}
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
