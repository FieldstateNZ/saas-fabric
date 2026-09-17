import type { ClientProductResponse } from '../api/catalogue-types'

/**
 * The client-level facts shown on both `ClientWorkspace`'s Overview and
 * Configuration tabs.
 *
 * One component so the two tabs cannot drift on which facts count as "the
 * client's own" versus product configuration — they show the same five
 * fields because they are answering the same question from two places in
 * the workspace.
 */
export function ClientFacts({ data }: { data: ClientProductResponse }) {
  return (
    <dl className="facts">
      <dt>Client ID</dt>
      <dd>{data.client.id}</dd>
      <dt>Legal name</dt>
      <dd>{data.product.legalName || 'Not set'}</dd>
      <dt>Region</dt>
      <dd>{data.product.region || 'Not set'}</dd>
      <dt>Timezone</dt>
      <dd>{data.product.timezone || 'Not set'}</dd>
      <dt>Identity realm</dt>
      <dd>{data.client.realm}</dd>
    </dl>
  )
}
