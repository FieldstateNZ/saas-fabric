import { useState } from 'react'

import { ClientDetail } from './components/ClientDetail'
import { ClientList } from './components/ClientList'
import { SignIn } from './components/SignIn'
import { useClients } from './hooks/useClients'
import { useSession } from './hooks/useSession'

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
 */
function Console() {
  const clients = useClients()
  const [selected, setSelected] = useState<string | null>(null)

  const current = clients.value?.find((client) => client.id === selected) ?? null

  return (
    <div className="app">
      <nav className="sidebar">
        <p className="sidebar__title">SaaS Fabric</p>
        <h2 className="sidebar__heading">Clients</h2>

        {clients.loading && <p className="empty">Loading...</p>}
        {clients.error !== null && <p className="error">{clients.error}</p>}
        {clients.value !== null && (
          <ClientList clients={clients.value} selected={selected} onSelect={setSelected} />
        )}
      </nav>

      <main className="main">
        {current === null ? (
          <p className="empty">Select a client to see its configuration.</p>
        ) : (
          <ClientDetail key={current.id} client={current} />
        )}
      </main>
    </div>
  )
}
