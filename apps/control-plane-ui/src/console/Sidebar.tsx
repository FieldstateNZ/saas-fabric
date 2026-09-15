import { Brand } from './Brand'
import { NAVIGATION, type Route } from './navigation'

/**
 * The console's primary navigation.
 *
 * The divider after index 4 groups "Client definition" onward under
 * "Platform", and the one after index 8 separates Settings from everything
 * above it — both are hard-coded positions in {@link NAVIGATION}'s order,
 * not a property on each entry. That is a real cost, not a convenience:
 * inserting a page anywhere but the end shifts every index after it, and
 * whoever adds one has to notice these two numbers and update them by hand.
 */
export function Sidebar({
  route,
  environment,
}: {
  route: Route
  environment?: string | undefined
}) {
  return (
    <aside className="fabric-sidebar">
      <a href="#/overview" className="brand-link" aria-label="SaaS Fabric overview">
        <Brand />
      </a>
      <nav aria-label="Main navigation">
        {NAVIGATION.map(([page, label], index) => (
          <div key={page}>
            {index === 4 && <p className="nav-group">Platform</p>}
            {index === 8 && <div className="nav-divider" />}
            <a
              href={`#/${page}`}
              aria-current={route.page === page ? 'page' : undefined}
              className={`nav-item${route.page === page ? ' nav-item--active' : ''}`}
            >
              {label}
            </a>
          </div>
        ))}
      </nav>
      <div className="operator">
        <span className="operator-avatar" aria-hidden="true">
          O
        </span>
        <div>
          <strong>Operator console</strong>
          <span>{environment ?? 'SaaS Fabric'} · operator</span>
        </div>
      </div>
    </aside>
  )
}
