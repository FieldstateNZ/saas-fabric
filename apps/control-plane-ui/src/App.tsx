import { useState } from 'react'

import { ClientDetail } from './components/ClientDetail'
import { ClientList } from './components/ClientList'
import { ConvergeButton } from './components/ConvergeButton'
import { IntegrationNotice } from './components/IntegrationNotice'
import { IntegrationsPanel } from './components/IntegrationsPanel'
import { PlatformNotManaged, PlatformPanel } from './components/PlatformPanel'
import { SignIn } from './components/SignIn'
import { useClients } from './hooks/useClients'
import { useIntegration } from './hooks/useIntegration'
import { usePlatform } from './hooks/usePlatform'
import { usePlatformIntegration } from './hooks/usePlatformIntegration'
import { useSession } from './hooks/useSession'

/** What the console is showing. */
type View = 'clients' | 'integrations'

/**
 * Where the Git host just sent this browser back to, if it did.
 *
 * Read once, at startup, before `useIntegration` strips it. An operator who
 * has just approved an application on GitHub should land on the panel that
 * asked them to, not on the client list wondering whether it worked.
 */
function landing(): View {
  const query = new URLSearchParams(window.location.search)
  const settled = ['platform', 'platform_error'].some((key) => query.has(key))

  return settled ? 'integrations' : 'clients'
}

/**
 * The SaaS Fabric operator console.
 *
 * # The vocabulary is the information architecture
 *
 * Clients, Identity, Domains -- not the names of the services that implement
 * them (section 17). An operator manages what SaaS Fabric promises, and which
 * platform service happens to deliver it is not something this console asks
 * them to know.
 *
 * # Deliberately one screen
 *
 * A list and a detail pane, no router, no framework beyond React. The first
 * increment of an operator console should be small enough that its correctness
 * is obvious; it grows a router when there is a second thing to route to.
 *
 * # Signing in comes first
 *
 * Nothing below renders until the operator has an identity — or until the API
 * says this deployment establishes one at the network boundary instead.
 */
export function App() {
  const session = useSession()

  if (session.state.status === 'checking') {
    return <p className="empty">Loading...</p>
  }

  if (session.state.status === 'signed-out') {
    return <SignIn error={session.state.error} onSignIn={session.signIn} />
  }

  return <Console />
}

/**
 * The console proper.
 *
 * Separated from [`App`] so that every hook below runs only once there is an
 * operator identity to run them for. Rendering the client list while signed
 * out would fire requests that can only be refused.
 *
 * # Two views, and still no router
 *
 * A second thing to look at is not yet a second URL. Integrations is where an
 * operator goes to establish authority and then leaves; giving it a path would
 * mean owning history, deep links and a redirect target for two callbacks that
 * already come back to `/`.
 */
function Console() {
  const integration = useIntegration()
  const platformApplication = usePlatformIntegration()
  const platform = usePlatform()
  const clients = useClients()
  const [selected, setSelected] = useState<string | null>(null)
  const [view, setView] = useState<View>(landing)

  const current = clients.value?.find((client) => client.id === selected) ?? null

  // When the platform cannot reach client configuration, the client list's own
  // failure is that same fact reported a second time. Showing both would send
  // an operator looking for two problems.
  const unreachable = integration.value !== null && integration.value.status !== 'connected'

  return (
    <div className="app">
      <nav className="sidebar">
        <p className="sidebar__title">SaaS Fabric</p>
        <h2 className="sidebar__heading">Clients</h2>

        {clients.loading && <p className="empty">Loading...</p>}
        {clients.error !== null && !unreachable && <p className="error">{clients.error}</p>}
        {clients.value !== null && (
          <ClientList clients={clients.value} selected={selected} onSelect={setSelected} />
        )}

        {!unreachable && <ConvergeButton />}

        <button
          type="button"
          className="sidebar__settings"
          onClick={() => {
            setView(view === 'integrations' ? 'clients' : 'integrations')
          }}
        >
          {view === 'integrations' ? 'Back to clients' : 'Integrations'}
        </button>
      </nav>

      <main className="main">
        {view === 'integrations' ? (
          <IntegrationsPanel
            clients={integration.value}
            platformApplication={platformApplication.value}
            platform={platform}
          />
        ) : (
          <>
            {integration.value !== null && <IntegrationNotice integration={integration.value} />}

            {/* Above the clients, because it is about this environment rather
                than about one client in it -- and because an operator who has
                just published something looks here first. */}
            {platform.error !== null && <p className="error">{platform.error}</p>}
            {platform.unmanaged && <PlatformNotManaged />}
            {platform.value !== null && <PlatformPanel platform={platform.value} />}

            {!unreachable &&
              (current === null ? (
                <p className="empty">Select a client to see its configuration.</p>
              ) : (
                <ClientDetail key={current.id} client={current} />
              ))}
          </>
        )}
      </main>
    </div>
  )
}
