import { useState } from 'react'

import type { ClientProductResponse } from '../api/catalogue-types'
import { Panel } from '../console/Panel'

/**
 * A preview of what a client's users would see, built from resolved
 * navigation and assigned plans.
 *
 * # A preview, not an enforcement point
 *
 * The banner says so, and it means it: this reads entitlements the same way
 * the client application eventually would, but it does not check them. The
 * client application is the thing that must refuse an unpermitted request —
 * hiding a navigation item here is a convenience for the operator reviewing
 * an assignment, not a security boundary.
 */
export function ClientShell({ data, onBack }: { data: ClientProductResponse; onBack: () => void }) {
  const [selected, setSelected] = useState('')

  return (
    <>
      <button onClick={onBack}>← Back to {data.client.displayName}</button>
      <p className="support-note">
        Client shell preview — navigation reflects the assigned plans. Application access checks
        still apply.
      </p>
      <div className="shell-preview">
        <aside>
          <h2>{data.client.displayName}</h2>
          {data.resolved.map((app) => (
            <section key={app.applicationId}>
              <h3>
                {
                  data.product.applications.find((a) => a.applicationId === app.applicationId)
                    ?.release.definition.name
                }
              </h3>
              {app.navigation.map((item) => (
                <button
                  key={item.route}
                  onClick={() => {
                    setSelected(`${item.label} · ${item.route}`)
                  }}
                >
                  {item.label}
                </button>
              ))}
            </section>
          ))}
        </aside>
        <div className="shell-content">
          <p className="eyebrow">{data.client.displayName}</p>
          <h1>Good morning</h1>
          {selected && <p role="status">Selected destination: {selected}</p>}
          {data.product.applications.map((app) => (
            <Panel key={app.applicationId} title={app.release.definition.name}>
              <div className="panel-body">
                <p>{app.release.definition.plans.find((p) => p.id === app.planId)?.name} plan</p>
                <p>{app.release.definition.description}</p>
              </div>
            </Panel>
          ))}
        </div>
      </div>
    </>
  )
}
