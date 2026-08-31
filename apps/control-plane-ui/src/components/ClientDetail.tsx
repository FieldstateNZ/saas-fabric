import { useState } from 'react'

import type { Client } from '../api/types'
import { IdentityPanel } from './IdentityPanel'
import { NotExposed } from './tabs/NotExposed'
import { Overview } from './tabs/Overview'
import { Secrets } from './tabs/Secrets'
import { CLIENT_TABS, type ClientTab } from './tabs/tabs'

/**
 * One client, and everything SaaS Fabric can say about it.
 *
 * # Why the tabs exist before their contents do
 *
 * The tab strip is the product surface: it states what an operator should be
 * able to see and manage about a client, and most of it is not built. Showing
 * the shape now is what makes the gaps concrete — each empty tab names the API
 * it needs, so the next piece of work is chosen by the screen rather than
 * guessed at (see docs/product/console-v0.md).
 */
export function ClientDetail({ client }: { client: Client }) {
  const [tab, setTab] = useState<ClientTab>('Overview')

  return (
    <article className="detail">
      <header className="detail__header">
        <h1>{client.displayName}</h1>
        <p className="detail__id">{client.id}</p>
      </header>

      <nav className="tabs" aria-label="Client sections">
        {CLIENT_TABS.map((name) => (
          <button
            key={name}
            type="button"
            className={name === tab ? 'tabs__tab tabs__tab--current' : 'tabs__tab'}
            aria-current={name === tab ? 'page' : undefined}
            onClick={() => {
              setTab(name)
            }}
          >
            {name}
          </button>
        ))}
      </nav>

      <Panel tab={tab} client={client} />
    </article>
  )
}

/** Whatever the chosen tab shows. */
function Panel({ tab, client }: { tab: ClientTab; client: Client }) {
  switch (tab) {
    case 'Overview':
      return <Overview client={client} />

    case 'Identity':
      return <IdentityPanel client={client} />

    case 'Secrets':
      return <Secrets client={client} />

    case 'Authorization':
      return (
        <NotExposed
          shows="the declared and live authorization model, its tuples, and a Check form"
          needs="The declared model is parsed from desired state but not served, and there is no control-plane path to OpenFGA yet."
        />
      )

    case 'Modules':
      return <NotExposed shows="which platform modules are enabled" needs="No module enablement model exists." />

    case 'Config':
      return (
        <NotExposed
          shows="this client's desired-state document exactly as Git holds it"
          needs="The API serves the parts it models, never the document itself."
        />
      )

    case 'Health':
      return (
        <NotExposed
          shows="provisioning and health for everything this client depends on"
          needs="Only reconciliation status is observed, and it is shown under Identity."
        />
      )

    default:
      return null
  }
}
