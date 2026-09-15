import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { IdentityPanel } from '../components/IdentityPanel'
import { Secrets } from '../components/tabs/Secrets'
import { ActivityTable } from './ActivityTable'
import { ClientApplicationsTab } from './ClientApplicationsTab'
import { ClientConfigurationTab } from './ClientConfigurationTab'
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
 */
export function ClientWorkspaceTab({
  tab,
  data,
  catalogue,
  onPreview,
}: {
  tab: string
  data: ClientProductResponse
  catalogue: Catalogue
  onPreview: () => void
}) {
  if (tab === 'Overview') {
    return <ClientOverviewTab data={data} onPreview={onPreview} />
  }
  if (tab === 'Applications') {
    return <ClientApplicationsTab data={data} />
  }
  if (tab === 'Configuration') {
    return <ClientConfigurationTab data={data} catalogue={catalogue} />
  }
  if (tab === 'Identity') {
    return <IdentityPanel client={data.client} />
  }
  if (tab === 'Secrets') {
    return <Secrets client={data.client} />
  }
  if (tab === 'Domains') {
    return <ClientDomainsTab data={data} />
  }
  if (tab === 'Activity') {
    return <ActivityTable activity={data.product.activity} />
  }
  if (tab === 'Health') {
    return <ClientHealthTab data={data} />
  }
  return null
}
