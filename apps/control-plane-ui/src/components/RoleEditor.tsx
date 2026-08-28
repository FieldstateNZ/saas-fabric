import { useState } from 'react'

/**
 * The roles the platform requires in every client realm.
 *
 * Held here so the console can say *why* a row has no remove button, rather
 * than letting an operator try and receive a refusal from the API. The API
 * refuses either way — this is a courtesy, not the enforcement.
 */
const REQUIRED_ROLES: readonly string[] = ['Client Realm Administrator', 'Client Realm User']

interface RoleEditorProps {
  readonly roles: readonly string[]
  readonly disabled: boolean
  readonly onChange: (roles: readonly string[]) => void
}

/** Adds and removes a client's realm roles. */
export function RoleEditor({ roles, disabled, onChange }: RoleEditorProps) {
  const [draft, setDraft] = useState('')

  const add = () => {
    const name = draft.trim()
    if (name === '' || roles.includes(name)) {
      return
    }
    onChange([...roles, name])
    setDraft('')
  }

  return (
    <div className="roles">
      <ul className="roles__list">
        {roles.map((role) => (
          <li key={role} className="roles__item">
            <span>{role}</span>
            {REQUIRED_ROLES.includes(role) ? (
              <span className="roles__required" title="Every client must have this role.">
                required
              </span>
            ) : (
              <button
                type="button"
                className="roles__remove"
                disabled={disabled}
                onClick={() => {
                  onChange(roles.filter((existing) => existing !== role))
                }}
              >
                Remove
              </button>
            )}
          </li>
        ))}
      </ul>

      <div className="roles__add">
        <label className="roles__label" htmlFor="new-role">
          Add a role
        </label>
        <input
          id="new-role"
          className="roles__input"
          value={draft}
          disabled={disabled}
          placeholder="Invoicing Approver"
          onChange={(event) => {
            setDraft(event.target.value)
          }}
        />
        <button type="button" disabled={disabled || draft.trim() === ''} onClick={add}>
          Add
        </button>
      </div>
    </div>
  )
}
