import type { ApplicationDefinition } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { Field } from './Field'

/**
 * The Entry points tab of `ApplicationWorkspace`: the hostname template an
 * assignment's sign-in callbacks are built from.
 *
 * The note is the same boundary `PlatformPanel` and `PHASE_ONE.md` state
 * elsewhere: declaring a hostname template registers PKCE S256 callbacks, it
 * does not prove the deployment behind that hostname is running, routed, or
 * holds a valid certificate — none of that is observed yet, and this tab does
 * not pretend otherwise.
 */
export function ApplicationEntryPointsTab({
  draft,
  onChange,
}: {
  draft: ApplicationDefinition
  onChange: (draft: ApplicationDefinition) => void
}) {
  return (
    <Panel title="Client entry points">
      <div className="panel-body">
        <Field
          label="Application hostname template"
          value={draft.domain}
          onChange={(domain) => {
            onChange({ ...draft, domain })
          }}
          hint="For example {client}.workspec.io. Leave empty to use the client’s primary host."
        />
        <p>
          Each assignment registers public sign-in callbacks using PKCE S256. Hostname declarations
          do not prove routing or certificate readiness.
        </p>
      </div>
    </Panel>
  )
}
