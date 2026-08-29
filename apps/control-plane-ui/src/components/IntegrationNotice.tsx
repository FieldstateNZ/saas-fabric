import type { Integration } from '../api/types'

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
  if (integration.status === 'connected') {
    return null
  }

  if (integration.status === 'not_configured') {
    return (
      <div className="notice">
        <h2 className="notice__heading">No client configuration connected</h2>
        <p className="notice__body">
          This platform is running, but it is not connected to where client configuration is
          kept &mdash; so there are no clients to show yet.
        </p>
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
    </div>
  )
}
