import type { ClientProductResponse } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { Status } from '../console/Status'

/**
 * The Domains tab of `ClientWorkspace`: every hostname declared for this
 * client.
 *
 * "Declared", not "live" — this console has no way to observe routing or
 * certificate state, so every domain gets the same neutral status regardless
 * of whether traffic could actually reach it today.
 */
export function ClientDomainsTab({ data }: { data: ClientProductResponse }) {
  return (
    <Panel title="Declared domains">
      <div className="panel-body">
        <ul className="domain-list">
          {data.client.hosts.map((host) => (
            <li key={host}>
              <span className="mono">{host}</span>
              <Status value="neutral">Declared</Status>
            </li>
          ))}
        </ul>
        <p>
          Manage hostnames through Configure client. Routing and certificate health require
          observations from the deployment system.
        </p>
      </div>
    </Panel>
  )
}
