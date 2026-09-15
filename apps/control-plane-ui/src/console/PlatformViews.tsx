import { PlatformPanel } from '../components/PlatformPanel'
import type { PlatformState } from '../hooks/usePlatform'
import { EmptyState } from './EmptyState'
import { PageHeader } from './PageHeader'
import { Panel } from './Panel'
import { Status } from './Status'

/**
 * The Components and Environments pages: two views of the same
 * `usePlatform` state, told apart by `environments`.
 *
 * The Environments view is a summary card — components, when they were last
 * checked — because that page also lists environment registrations that are
 * not platform state at all. Components shows the full {@link PlatformPanel}
 * instead, since that page has nothing else on it.
 */
export function PlatformViews({
  platform,
  environments = false,
}: {
  platform: PlatformState
  environments?: boolean
}) {
  return (
    <>
      <PageHeader
        title={environments ? 'Environments' : 'Components'}
        description={
          environments
            ? 'The environment managed by this deployment of SaaS Fabric.'
            : 'Desired versions, update policies, and controls for your platform.'
        }
      />
      {platform.loading && <p role="status">Loading platform…</p>}
      {platform.error && (
        <p role="alert" className="error">
          {platform.error}
        </p>
      )}
      {platform.unmanaged && (
        <EmptyState title="Connect platform management">
          <p>Connect an integration to see this environment and its components.</p>
          <a className="primary-link" href="#/integrations">
            Manage integrations →
          </a>
        </EmptyState>
      )}
      {platform.value &&
        (environments ? (
          <Panel
            title={platform.value.environment}
            action={<Status value="neutral">Current environment</Status>}
          >
            <div className="panel-body">
              <dl className="facts">
                <dt>Components</dt>
                <dd>{platform.value.components.length}</dd>
                <dt>Last check</dt>
                <dd>
                  {platform.value.lastCheck
                    ? new Date(platform.value.lastCheck.atUnixSeconds * 1000).toLocaleString()
                    : 'Not checked yet'}
                </dd>
                <dt>Running state</dt>
                <dd>Not observed</dd>
              </dl>
              <a href="#/components">Manage components →</a>
            </div>
          </Panel>
        ) : (
          <PlatformPanel platform={platform.value} />
        ))}
    </>
  )
}
