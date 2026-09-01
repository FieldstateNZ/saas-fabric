import { clientConfiguration, platformManagement } from '../api/client'
import type { ConnectionState, Integration, PlatformIntegration } from '../api/types'
import type { PlatformState } from '../hooks/usePlatform'
import { IntegrationCard } from './IntegrationCard'

/**
 * The two applications this platform holds, and what an operator does about
 * each.
 *
 * # Two panels, not one list
 *
 * They are separate product concepts, not two rows of a generic integrations
 * table. An operator connects "where client configuration is kept" or "where
 * this platform's own composition is kept"; neither is an instance of a third
 * thing, and naming one would invite a fourth.
 */
interface IntegrationsPanelProps {
  readonly clients: Integration | null
  readonly platformApplication: PlatformIntegration | null
  readonly platform: PlatformState
}

export function IntegrationsPanel(props: IntegrationsPanelProps) {
  const { clients, platformApplication, platform } = props

  return (
    <div className="integrations">
      <h2 className="integrations__heading">Integrations</h2>

      {clients !== null && (
        <IntegrationCard
          name="Client Configuration"
          purpose="the clients this platform manages"
          state={clientState(clients)}
          endpoints={clientConfiguration}
          application={clients.application}
          diagnostic={clientDiagnostic(clients)}
          unmanaged="This deployment states its own client repository."
        />
      )}

      {platformApplication !== null && (
        <IntegrationCard
          name="Platform Management"
          purpose="this platform's own composition"
          state={platformStateOf(platformApplication, platform)}
          endpoints={platformManagement}
          application={platformApplication.application}
          diagnostic={platform.error}
          unmanaged="This deployment manages no environment."
        />
      )}
    </div>
  )
}

/**
 * What state client configuration is in.
 *
 * `invalid` and `error` both land on `unavailable`: they differ in *why* the
 * platform cannot read, and not in what an operator does next.
 */
function clientState(integration: Integration): ConnectionState {
  if (!integration.managed) {
    return 'not-managed'
  }

  switch (integration.status) {
    case 'connected':
      return 'connected'
    case 'not_configured':
      return 'not-connected'
    default:
      return 'unavailable'
  }
}

/** Why client configuration is unreadable, when it is. */
function clientDiagnostic(integration: Integration): string | null {
  if (integration.status === 'invalid') {
    return 'The application’s access was refused. It may have been revoked or removed.'
  }

  if (integration.status === 'error') {
    return 'The platform is connected but cannot read client configuration right now.'
  }

  return null
}

/**
 * What state platform management is in, from both halves of the answer.
 *
 * The order matters. An application that does not exist yet is *not connected*
 * however the platform route answers, because there is nothing for it to have
 * been failing at. Only once there is one does the platform route decide
 * between connected and unavailable.
 */
function platformStateOf(
  integration: PlatformIntegration,
  platform: PlatformState,
): ConnectionState {
  if (!integration.managed) {
    return 'not-managed'
  }

  if (integration.application === null || integration.application.repository === null) {
    return 'not-connected'
  }

  // A repository was chosen and the binding still holds nothing. Whatever went
  // wrong, this is a connected integration that does not work — which is
  // exactly what must not read as "not connected".
  if (platform.unmanaged) {
    return 'unavailable'
  }

  return platform.error === null && platform.value !== null ? 'connected' : 'unavailable'
}
