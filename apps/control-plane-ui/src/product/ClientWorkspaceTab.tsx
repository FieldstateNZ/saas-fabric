import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { IdentityPanel } from '../components/IdentityPanel'
import { Secrets } from '../components/tabs/Secrets'
import { ActivityTable } from './ActivityTable'
import { ClientApplicationsTab } from './ClientApplicationsTab'
import type { ClientTab } from './clientWorkspaceTabs'
import { ClientConfigurationTab } from './ClientConfigurationTab'
import { ClientDataTab } from './ClientDataTab'
import { ClientDomainsTab } from './ClientDomainsTab'
import { ClientHealthTab } from './ClientHealthTab'
import { ClientOverviewTab } from './ClientOverviewTab'

/**
 * Chooses which section of a client's workspace `ClientWorkspace`'s current
 * tab shows.
 *
 * Identity and Secrets delegate straight to the pre-existing `IdentityPanel`
 * and `Secrets` components rather than a wrapper of their own — both
 * already do everything this tab needs, and a wrapper would add a layer
 * with nothing to say.
 *
 * The `switch` has no `default` for its nine real cases, the way
 * `ApplicationWorkspaceTab`'s does not — see that file's doc for why
 * `exhaustive: never = tab` is there.
 */
export function ClientWorkspaceTab({
  tab,
  data,
  catalogue,
  onPreview,
  previewDisabled = false,
}: {
  tab: ClientTab
  data: ClientProductResponse
  catalogue: Catalogue
  onPreview: () => void
  previewDisabled?: boolean
}) {
  switch (tab) {
    case 'Overview':
      return <ClientOverviewTab data={data} onPreview={onPreview} previewDisabled={previewDisabled} />

    case 'Applications':
      return <ClientApplicationsTab data={data} />

    case 'Configuration':
      return <ClientConfigurationTab data={data} catalogue={catalogue} />

    case 'Data':
      return <ClientDataTab data={data} />

    case 'Identity':
      return <IdentityPanel client={data.client} />

    case 'Secrets':
      return <Secrets client={data.client} />

    case 'Domains':
      return <ClientDomainsTab data={data} />

    case 'Activity':
      return <ActivityTable activity={data.product.activity} />

    case 'Health':
      return <ClientHealthTab data={data} />

    default: {
      const exhaustive: never = tab
      return exhaustive
    }
  }
}
