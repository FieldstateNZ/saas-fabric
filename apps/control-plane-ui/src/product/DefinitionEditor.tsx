import { useEffect, useState } from 'react'

import { PageHeader } from '../console/PageHeader'
import { Panel } from '../console/Panel'
import { FieldDefinitions } from './FieldDefinitions'
import { SaveNotice } from './SaveNotice'
import type { CatalogueState } from './useCatalogue'

/**
 * Editing the client fields every client shares, alongside the fixed core
 * ones (client ID, display name, legal name, region, timezone, hostnames)
 * this page does not offer, because they are not optional per client the way
 * these definitions are.
 *
 * Saving increments `definitionVersion` on the control plane's side, and an
 * existing client keeps its current field values until an operator
 * reconfigures it — this save does not retroactively touch anybody.
 */
export function DefinitionEditor({ state }: { state: CatalogueState }) {
  const [fields, setFields] = useState(state.value?.catalogue.clientFields ?? [])
  const [success, setSuccess] = useState<string | null>(null)

  useEffect(() => {
    setFields(state.value?.catalogue.clientFields ?? [])
  }, [state.value])

  const version = state.value?.catalogue.definitionVersion ?? 0

  return (
    <>
      <PageHeader
        title="Client definition"
        description={`Shared client configuration · version ${String(version)}`}
      />
      <SaveNotice
        error={state.saveError}
        success={success}
        onReload={state.conflict ? state.refresh : undefined}
      />
      <Panel title="Core fields">
        <div className="panel-body">
          <p>
            Client ID, display name, legal name, region, timezone and hostnames are part of every
            client. Define additional non-secret fields below.
          </p>
        </div>
      </Panel>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          void state.save({ action: 'saveDefinition', fields }).then((saved) => {
            if (saved) {
              setSuccess(
                'Client definition saved. Existing clients keep their current values until reconfigured.',
              )
            }
          })
        }}
      >
        <fieldset disabled={state.saving}>
          <FieldDefinitions fields={fields} onChange={setFields} />
          <button className="primary-button" type="submit">
            {state.saving ? 'Saving…' : 'Save client definition'}
          </button>
        </fieldset>
      </form>
    </>
  )
}
