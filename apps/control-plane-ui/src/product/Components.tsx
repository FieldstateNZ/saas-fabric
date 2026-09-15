import type { Catalogue } from '../api/catalogue-types'
import type { PlatformState } from '../hooks/usePlatform'
import { PlatformViews } from '../console/PlatformViews'
import { Panel } from '../console/primitives'
export function Components({ catalogue, platform }: { catalogue: Catalogue; platform: PlatformState }) {
  return <><PlatformViews platform={platform} /><h2>Application components</h2>{catalogue.applications.map((app) => <Panel key={app.id} title={app.draft.name} action={<a href={`#/applications/${encodeURIComponent(app.id)}`}>Edit components →</a>}>
    <div className="table-wrap"><table className="fabric-table"><thead><tr><th>Component</th><th>Kind</th><th>Reference</th><th>Version</th><th>Policy</th></tr></thead><tbody>{app.draft.components.map((component) => <tr key={component.id}><td>{component.name}</td><td>{component.kind}</td><td className="mono">{component.reference}</td><td>{component.version || 'Platform managed'}</td><td>{component.policy}</td></tr>)}</tbody></table>
      {app.draft.components.length === 0 && <p className="panel-body">No components defined yet.</p>}</div></Panel>)}</>
}
