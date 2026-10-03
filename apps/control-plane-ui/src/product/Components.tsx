import type { Catalogue } from '../api/catalogue-types'
import type { ApplicationComponent } from '../api/component-types'
import { Panel } from '../console/Panel'
import { PlatformViews } from '../console/PlatformViews'
import type { PlatformState } from '../hooks/usePlatform'

/**
 * What a component's version cell says: a described component's selected
 * version tag, as its resolution records it; nothing for a capability,
 * which the platform provides; and "Not pinned" for an authored component
 * with no version yet.
 */
function versionOf(component: ApplicationComponent): string {
  if (component.resolution !== undefined) {
    return component.resolution.version
  }
  return component.kind === 'capability' ? 'Platform capability' : component.version || 'Not pinned'
}

/**
 * The Components page: the platform's own components (see
 * {@link PlatformViews}), plus every application's declared components,
 * read-only here.
 *
 * A described component shows the repository and version tag its
 * resolution records -- what was selected, not a claim about what runs.
 *
 * Each application links back to its own workspace to edit — this page is
 * for surveying components across the whole catalogue, not for changing any
 * one of them.
 */
export function Components({
  catalogue,
  platform,
}: {
  catalogue: Catalogue
  platform: PlatformState
}) {
  return (
    <>
      <PlatformViews platform={platform} />
      <h2>Application components</h2>
      {catalogue.applications.map((app) => (
        <Panel
          key={app.id}
          title={app.draft.name}
          action={<a href={`#/applications/${encodeURIComponent(app.id)}`}>Edit components →</a>}
        >
          <div className="table-wrap">
            <table className="fabric-table">
              <thead>
                <tr>
                  <th>Component</th>
                  <th>Kind</th>
                  <th>Reference</th>
                  <th>Version</th>
                  <th>Policy</th>
                </tr>
              </thead>
              <tbody>
                {app.draft.components.map((component) => (
                  <tr key={component.id}>
                    <td>{component.name}</td>
                    <td>{component.kind}</td>
                    <td className="mono">
                      {component.resolution?.repository ?? component.reference}
                    </td>
                    <td>{versionOf(component)}</td>
                    <td>{component.policy}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {app.draft.components.length === 0 && (
              <p className="panel-body">No components defined yet.</p>
            )}
          </div>
        </Panel>
      ))}
    </>
  )
}
