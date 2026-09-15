import type { ReactNode } from 'react'

import { IntegrationsPanel } from '../components/IntegrationsPanel'
import type { Catalogue, ProductApplication } from '../api/catalogue-types'
import type { Client, Integration, PlatformIntegration } from '../api/types'
import type { Loadable } from '../hooks/useClients'
import type { PlatformState } from '../hooks/usePlatform'
import { ApplicationCatalogue } from '../product/ApplicationCatalogue'
import { ApplicationWorkspace } from '../product/ApplicationWorkspace'
import { ClientForm } from '../product/ClientForm'
import { ClientWorkspace } from '../product/ClientWorkspace'
import { Components } from '../product/Components'
import { DefinitionEditor } from '../product/DefinitionEditor'
import { Environments } from '../product/Environments'
import { ProductActivityFeed } from '../product/ProductActivityFeed'
import { SettingsEditor } from '../product/SettingsEditor'
import type { CatalogueState } from '../product/useCatalogue'
import { ClientDirectory } from './ClientDirectory'
import { Dashboard } from './Dashboard'
import { EmptyState } from './EmptyState'
import { PageHeader } from './PageHeader'
import { Reconciliation } from './Reconciliation'
import type { Route } from './navigation'
import type { Inventory } from './useInventory'

/**
 * Every shared read `Console` holds, and the one derived value
 * (`unreachable`) a page needs but does not load itself.
 */
interface ConsoleRouteProps {
  readonly route: Route
  readonly product: Catalogue | undefined
  readonly app: ProductApplication | undefined
  readonly catalogue: CatalogueState
  readonly clients: Loadable<readonly Client[]> & { refresh: () => void }
  readonly platform: PlatformState
  readonly platformApplication: Loadable<PlatformIntegration>
  readonly integration: Loadable<Integration>
  readonly inventory: Inventory
  readonly unreachable: boolean
}

/**
 * Chooses what `Console`'s main content area shows for the current route.
 *
 * Split out from `Console` itself so the shell — sidebar, skip link, the
 * loading and error banners every page can show — stays readable as its own
 * concern, separate from the one-page-per-route decision made here. This
 * file itself starts nothing: every prop below is state `Console` already
 * loaded. The pages it renders are a different matter — `ClientWorkspace`
 * reads a client's product on mount, `ProductActivityFeed` reads activity,
 * and more than one of these is its own request the moment it mounts.
 *
 * This file sits in file-size-policy.md's 121-150 line band, and stays a
 * `switch` rather than a further-split lookup table on purpose: a route is
 * a genuine decision with real per-case logic (a nested ternary for
 * `applications`, an assembled fragment for `integrations`), not a uniform
 * mapping that would compress into a table. Its own props are already a
 * ten-field bag for the same reason `ApplicationWorkspaceTab`'s are — every
 * page needs a different subset of what `Console` loaded, and there is no
 * narrower prop shape that would not just move the bag one level up.
 */
export function ConsoleRoute({
  route,
  product,
  app,
  catalogue,
  clients,
  platform,
  platformApplication,
  integration,
  inventory,
  unreachable,
}: ConsoleRouteProps): ReactNode {
  switch (route.page) {
    case 'overview':
      return (
        <Dashboard
          clients={clients.value}
          platform={platform.value}
          inventory={inventory}
          catalogue={product}
        />
      )

    case 'clients':
      return route.clientId === null ? (
        <ClientDirectory clients={clients.value ?? []} inventory={inventory} />
      ) : (
        product && (
          <ClientWorkspace
            key={route.clientId}
            id={route.clientId}
            catalogue={product}
            onSaved={clients.refresh}
          />
        )
      )

    case 'create-client':
      return product && <ClientForm catalogue={product} onSaved={clients.refresh} />

    case 'applications':
      return route.applicationId ? (
        app ? (
          <ApplicationWorkspace key={app.id} app={app} state={catalogue} />
        ) : (
          <EmptyState title="Application unavailable">
            <p>Select an application from the catalogue.</p>
          </EmptyState>
        )
      ) : (
        <ApplicationCatalogue state={catalogue} />
      )

    case 'components':
      return product && <Components catalogue={product} platform={platform} />

    case 'environments':
      return <Environments state={catalogue} platform={platform} />

    case 'definition':
      return <DefinitionEditor state={catalogue} />

    case 'settings':
      return <SettingsEditor state={catalogue} />

    case 'reconciliation':
      return (
        <>
          <Reconciliation
            inventory={inventory}
            connected={!unreachable && clients.value !== null}
          />
          <ProductActivityFeed />
        </>
      )

    case 'integrations':
      return (
        <>
          <PageHeader
            title="Integrations"
            description="Connections that keep your platform and client configuration in sync."
          />
          <IntegrationsPanel
            clients={integration.value}
            platformApplication={platformApplication.value}
            platform={platform}
          />
          {integration.error && (
            <p className="error" role="alert">
              {integration.error}
            </p>
          )}
          {platformApplication.error && (
            <p className="error" role="alert">
              {platformApplication.error}
            </p>
          )}
        </>
      )

    default:
      return null
  }
}
