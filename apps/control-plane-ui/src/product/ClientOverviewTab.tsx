import type { ClientProductResponse } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { Status } from '../console/Status'
import { ClientFacts } from './ClientFacts'

/**
 * The Overview tab of `ClientWorkspace`: three headline metrics, the
 * client's own facts, and a way into the shell preview.
 */
export function ClientOverviewTab({
  data,
  onPreview,
}: {
  data: ClientProductResponse
  onPreview: () => void
}) {
  return (
    <>
      <div className="metrics">
        <div className="metric">
          <span>Applications</span>
          <strong>{data.product.applications.length}</strong>
        </div>
        <div className="metric">
          <span>Domains</span>
          <strong>{data.client.hosts.length}</strong>
        </div>
        <div className="metric">
          <span>Identity</span>
          <p>
            <Status value={data.reconciliation.status} />
          </p>
        </div>
      </div>
      <Panel title="Client details">
        <div className="panel-body">
          <ClientFacts data={data} />
          <button onClick={onPreview}>Preview client shell →</button>
        </div>
      </Panel>
    </>
  )
}
