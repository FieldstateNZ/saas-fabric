import { Brand } from './primitives'
import { NAVIGATION, type Route } from './navigation'

export function Sidebar({ route, environment }: { route: Route; environment?: string | undefined }) {
  return <aside className="fabric-sidebar"><a href="#/overview" className="brand-link" aria-label="SaaS Fabric overview"><Brand /></a>
    <nav aria-label="Main navigation">
      {NAVIGATION.map(([page, label], index) => <div key={page}>
        {index === 4 && <p className="nav-group">Platform</p>}
        {index === 8 && <div className="nav-divider" />}
        <a href={`#/${page}`} aria-current={route.page === page ? 'page' : undefined}
          className={`nav-item${route.page === page ? ' nav-item--active' : ''}`}>{label}</a>
      </div>)}
    </nav>
    <div className="operator"><span className="operator-avatar" aria-hidden="true">O</span>
      <div><strong>Operator console</strong><span>{environment ?? 'SaaS Fabric'} · operator</span></div>
    </div>
  </aside>
}
