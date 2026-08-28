import type { Client } from '../api/types'
import { IdentityPanel } from './IdentityPanel'

/** One client: what it is, and what its identity should be. */
export function ClientDetail({ client }: { client: Client }) {
  return (
    <article className="detail">
      <header className="detail__header">
        <h1>{client.displayName}</h1>
        <p className="detail__id">{client.id}</p>
      </header>

      <section className="overview">
        <h2>Overview</h2>
        <dl className="overview__fields">
          <dt>Domains</dt>
          <dd>{client.hosts.length === 0 ? 'None yet' : client.hosts.join(', ')}</dd>

          <dt>Realm</dt>
          <dd>{client.realm}</dd>
        </dl>
      </section>

      <IdentityPanel client={client} />
    </article>
  )
}
