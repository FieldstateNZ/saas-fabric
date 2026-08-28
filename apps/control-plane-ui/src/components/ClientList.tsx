import type { Client } from '../api/types'

interface ClientListProps {
  readonly clients: readonly Client[]
  readonly selected: string | null
  readonly onSelect: (clientId: string) => void
}

/** Every client the platform manages. */
export function ClientList({ clients, selected, onSelect }: ClientListProps) {
  if (clients.length === 0) {
    // Distinct from a failed load on purpose: "no clients" and "the platform
    // could not tell us" look identical if both render an empty list, and only
    // one of them is somebody's problem right now.
    return <p className="empty">No clients are defined yet.</p>
  }

  return (
    <ul className="clients">
      {clients.map((client) => (
        <li key={client.id}>
          <button
            type="button"
            className={
              client.id === selected ? 'clients__item clients__item--selected' : 'clients__item'
            }
            onClick={() => {
              onSelect(client.id)
            }}
          >
            <span className="clients__name">{client.displayName}</span>
            <span className="clients__id">{client.id}</span>
          </button>
        </li>
      ))}
    </ul>
  )
}
