import type { ApplicationDefinition } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { Field } from './Field'

/**
 * The Definition tab of `ApplicationWorkspace`: an application's name and
 * description.
 *
 * The note beneath the fields exists because everything else in the
 * workspace — components, features, plans, fields, navigation — is edited on
 * its own tab and is easy to forget is part of the same draft. Publishing
 * copies all of it, together, into one immutable release.
 */
export function ApplicationDefinitionTab({
  appId,
  draft,
  onChange,
}: {
  appId: string
  draft: ApplicationDefinition
  onChange: (draft: ApplicationDefinition) => void
}) {
  return (
    <Panel title="Application definition">
      <div className="panel-body form-grid">
        <Field
          label="Name"
          value={draft.name}
          required
          onChange={(name) => {
            onChange({ ...draft, name })
          }}
        />
        <Field
          label="Description"
          value={draft.description}
          onChange={(description) => {
            onChange({ ...draft, description })
          }}
        />
        <p className="support-note">
          Application ID: {appId}. Published versions contain the complete component, feature, plan,
          field and navigation definitions.
        </p>
      </div>
    </Panel>
  )
}
