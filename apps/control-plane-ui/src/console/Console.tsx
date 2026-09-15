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
 * README.md's "What it talks to". The client list, catalogue, integrations
 * and platform state stay mounted for the whole session and are not
 * re-fetched on navigation. The identity inventory is the exception: this
 * effect re-reads it on entering a page that shows it, because an edit made
 * elsewhere (an identity change, a client write) can leave it stale.
 * `Reconciliation`'s own "Refresh observations" button reads the same
 * inventory on demand — this effect is what refreshes it on arrival,
 * without an operator having to ask.
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
    // Only the pages that actually render `inventory` need a fresh read on
    // arrival — re-reading every client's identity on every route change
    // (Settings, Definition, an application's editor) would cost a request
    // wave for pages that show none of it.
    const showsInventory =
      route.page === 'overview' ||
      route.page === 'reconciliation' ||
      (route.page === 'clients' && route.clientId === null)

    if (showsInventory) {
      refreshInventory()
    }
  }, [route.page, route.clientId, refreshInventory])

  const clearCatalogueSaveError = catalogue.clearSaveError
  useEffect(() => {
    clearCatalogueSaveError()
  }, [route.page, clearCatalogueSaveError])

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
          {catalogue.loadError && route.page !== 'integrations' && (
            <div className="error" role="alert">
              {catalogue.loadError}
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
