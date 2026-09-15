import { useCatalogue } from '../product/useCatalogue'
import { ApplicationCatalogue } from '../product/ApplicationCatalogue'
import { ApplicationWorkspace } from '../product/ApplicationWorkspace'
import { ClientForm } from '../product/ClientForm'
import { ClientWorkspace } from '../product/ClientWorkspace'
import { DefinitionEditor, SettingsEditor } from '../product/DefinitionSettings'
import { Environments } from '../product/Environments'
import { Components } from '../product/Components'
import { ProductActivityFeed } from '../product/ProductActivity'
import { useEffect, useRef } from 'react'
import { IntegrationNotice } from '../components/IntegrationNotice'
import { IntegrationsPanel } from '../components/IntegrationsPanel'
import { useClients } from '../hooks/useClients'
import { useIntegration } from '../hooks/useIntegration'
import { usePlatform } from '../hooks/usePlatform'
import { usePlatformIntegration } from '../hooks/usePlatformIntegration'
import { ClientDirectory } from './ClientDirectory'
import { Dashboard } from './Dashboard'
import { NAVIGATION, useRoute } from './navigation'
import { EmptyState, PageHeader } from './primitives'
import { Reconciliation } from './Reconciliation'
import { Sidebar } from './Sidebar'
import { useInventory } from './useInventory'

/** Shared reads stay mounted across navigation; every operation uses the control-plane API. */
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
  useEffect(() => { refreshInventory() }, [route.page, route.clientId, refreshInventory])
  const unreachable = integration.value !== null && integration.value.status !== 'connected'
  useEffect(() => {
    document.title = `${NAVIGATION.find(([page]) => page === route.page)?.[1] ?? 'Overview'} · SaaS Fabric`
    main.current?.focus()
    window.scrollTo({ top: 0 })
  }, [route.page, route.clientId, route.applicationId])

  let content
  switch (route.page) {
    case 'overview': content = <Dashboard clients={clients.value} platform={platform.value} inventory={inventory} catalogue={product} />; break
    case 'clients': content = route.clientId === null
      ? <ClientDirectory clients={clients.value ?? []} inventory={inventory} />
      : product && <ClientWorkspace key={route.clientId} id={route.clientId} catalogue={product} onSaved={clients.refresh} />; break
    case 'create-client': content = product && <ClientForm catalogue={product} onSaved={clients.refresh} />; break
    case 'applications': content = route.applicationId
      ? app ? <ApplicationWorkspace key={app.id} app={app} state={catalogue} /> : <EmptyState title="Application unavailable"><p>Select an application from the catalogue.</p></EmptyState>
      : <ApplicationCatalogue state={catalogue} />; break
    case 'components': content = product && <Components catalogue={product} platform={platform} />; break
    case 'environments': content = <Environments state={catalogue} platform={platform} />; break
    case 'definition': content = <DefinitionEditor state={catalogue} />; break
    case 'settings': content = <SettingsEditor state={catalogue} />; break
    case 'reconciliation': content = <><Reconciliation inventory={inventory} connected={!unreachable && clients.value !== null} /><ProductActivityFeed /></>; break
    case 'integrations': content = <><PageHeader title="Integrations" description="Connections that keep your platform and client configuration in sync." />
      <IntegrationsPanel clients={integration.value} platformApplication={platformApplication.value} platform={platform} />
      {integration.error && <p className="error" role="alert">{integration.error}</p>}
      {platformApplication.error && <p className="error" role="alert">{platformApplication.error}</p>}</>; break
  }
  const clientPage = ['overview', 'clients', 'applications', 'reconciliation'].includes(route.page)
  return <div className="fabric-app"><a className="skip-link" href="#main-content" onClick={(event) => { event.preventDefault(); main.current?.focus() }}>Skip to content</a>
    <Sidebar route={route} environment={product?.settings.platformName ?? platform.value?.environment} />
    <main ref={main} id="main-content" className="fabric-main" tabIndex={-1}><div className="content-width">
      {clientPage && integration.value && <IntegrationNotice integration={integration.value} />}
      {clientPage && clients.error && !unreachable && <p className="error" role="alert">{clients.error}</p>}
      {catalogue.error && route.page !== 'integrations' && <div className="error" role="alert">{catalogue.error}<button onClick={catalogue.refresh}>Retry catalogue</button></div>}
      {catalogue.loading && !product && route.page !== 'integrations' ? <p role="status">Loading application catalogue…</p> : content}

    </div></main>
  </div>
}
