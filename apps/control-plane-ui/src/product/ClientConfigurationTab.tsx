import type { Catalogue, ClientProductResponse } from '../api/catalogue-types'
import { Panel } from '../console/Panel'
import { ClientFacts } from './ClientFacts'

/**
 * The Configuration tab of `ClientWorkspace`: the client's own facts, plus
 * every shared definition field's current value.
 */
export function ClientConfigurationTab({
  data,
  catalogue,
}: {
  data: ClientProductResponse
  catalogue: Catalogue
}) {
  return (
    <Panel title={`Client definition v${String(data.product.definitionVersion)}`}>
      <div className="panel-body">
        <ClientFacts data={data} />
        <dl className="facts">
          {Object.entries(data.product.configuration).map(([key, value]) => (
            <div className="fact-pair" key={key}>
              <dt>{catalogue.clientFields.find((f) => f.key === key)?.label ?? key}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      </div>
    </Panel>
  )
}
