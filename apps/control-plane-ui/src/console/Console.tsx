import { useEffect, useRef } from 'react'

import { IntegrationNotice } from '../components/IntegrationNotice'
import { useClients } from '../hooks/useClients'
import { useIntegration } from '../hooks/useIntegration'
import { usePlatform } from '../hooks/usePlatform'
import { usePlatformIntegration } from '../hooks/usePlatformIntegration'
import { useCatalogue } from '../product/useCatalogue'
import { ConsoleRoute } from './ConsoleRoute'
import { NAVIGATION, useRoute } from './navigation'
import { Sidebar } from './Sidebar'
import { useInventory } from './useInventory'

/**
 * The console shell: every shared read, kept mounted across navigation, and
 * the sidebar, focus management, and page banners around whatever
 * {@link ConsoleRoute} decides to show.
 *
 * Every one of the hooks here loads from the control-plane API — see
 * README.md's "What it talks to" — and stays mounted for the whole session
 * rather than being loaded per page, so switching pages never re-fetches
 * data the console already has.
 */
export function Console() {
  const route = useRoute()
  const clients = useClients()
  const catalogue = useCatalogue()
  const product = catalogue.value?.catalogue
  const app = product?.applications.find((a) => a.id === route.applicationId)
  const integration = useIntegration()
  const platformApplication = usePlatformIntegration()
  const platform = usePlatform()
  const inventory = useInventory(clients.value)
  const main = useRef<HTMLElement>(null)

  const refreshInventory = inventory.refresh
  useEffect(() => {
    refreshInventory()
  }, [route.page, route.clientId, refreshInventory])

  const unreachable = integration.value !== null && integration.value.status !== 'connected'

  useEffect(() => {
    document.title = `${NAVIGATION.find(([page]) => page === route.page)?.[1] ?? 'Overview'} · SaaS Fabric`
    main.current?.focus()
    window.scrollTo({ top: 0 })
  }, [route.page, route.clientId, route.applicationId])

  const clientPage = ['overview', 'clients', 'applications', 'reconciliation'].includes(route.page)

  return (
    <div className="fabric-app">
      <a
        className="skip-link"
        href="#main-content"
        onClick={(event) => {
          event.preventDefault()
          main.current?.focus()
        }}
      >
        Skip to content
      </a>
      <Sidebar
        route={route}
        environment={product?.settings.platformName ?? platform.value?.environment}
      />
      <main ref={main} id="main-content" className="fabric-main" tabIndex={-1}>
        <div className="content-width">
          {clientPage && integration.value && <IntegrationNotice integration={integration.value} />}
          {clientPage && clients.error && !unreachable && (
            <p className="error" role="alert">
              {clients.error}
            </p>
          )}
          {catalogue.error && route.page !== 'integrations' && (
            <div className="error" role="alert">
              {catalogue.error}
              <button onClick={catalogue.refresh}>Retry catalogue</button>
            </div>
          )}
          {catalogue.loading && !product && route.page !== 'integrations' ? (
            <p role="status">Loading application catalogue…</p>
          ) : (
            <ConsoleRoute
              route={route}
              product={product}
              app={app}
              catalogue={catalogue}
              clients={clients}
              platform={platform}
              platformApplication={platformApplication}
              integration={integration}
              inventory={inventory}
              unreachable={unreachable}
            />
          )}
        </div>
      </main>
    </div>
  )
}
