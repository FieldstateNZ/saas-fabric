import type { Integration } from '../api/types'
import { GitIntegration } from './GitIntegration'
import { RepositoryPicker } from './RepositoryPicker'

/**
 * What the console says about the platform's connection to desired state.
 *
 * Three states, three different things for an operator to do — which is the
 * whole reason the control plane reports them separately rather than as one
 * "broken" flag.
 */
interface IntegrationNoticeProps {
  readonly integration: Integration
}

export function IntegrationNotice({ integration }: IntegrationNoticeProps) {
  // Connected, but the installation reaches several repositories and nobody
  // has said which one holds client configuration. The platform declines to
  // guess; this is where the operator answers.
  const undecided =
    integration.application?.installed === true && integration.application.repository === null

  if (integration.status === 'connected' && !undecided) {
    return null
  }

  if (undecided) {
    return (
      <div className="notice">
        <h2 className="notice__heading">Choose where client configuration lives</h2>
        <p className="notice__body">
          The application can reach more than one repository. SaaS Fabric will not guess which one
          holds client configuration.
        </p>
        <RepositoryPicker />
      </div>
    )
  }

  if (integration.status === 'not_configured') {
    return (
      <div className="notice">
        <h2 className="notice__heading">No client configuration connected</h2>
        <p className="notice__body">
          This platform is running, but it is not connected to where client configuration is
          kept &mdash; so there are no clients to show yet.
        </p>
        <GitIntegration integration={integration} />
      </div>
    )
  }

  const needsReconnect = integration.status === 'invalid'

  return (
    <div className="notice notice--warning">
      <h2 className="notice__heading">
        {needsReconnect ? 'Connection needs attention' : 'Client configuration is unreadable'}
      </h2>
      <p className="notice__body">
        {needsReconnect
          ? 'The platform is connected, but its access has been refused. It may have been revoked or removed.'
          : 'The platform is connected but cannot read client configuration right now.'}
        {integration.connection !== null && ` Connected to ${integration.connection}.`}
      </p>
      {integration.last_success_at !== null && (
        <p className="notice__detail">
          Last read successfully {new Date(integration.last_success_at * 1000).toLocaleString()}.
        </p>
      )}
      {needsReconnect && <GitIntegration integration={integration} />}
    </div>
  )
}
