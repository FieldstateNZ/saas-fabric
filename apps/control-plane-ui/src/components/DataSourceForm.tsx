import { useState } from 'react'

import type { DataSource, DataSourceInput } from '../api/data-source-types'
import { Check } from '../product/Check'
import { Field } from '../product/Field'
import { Select } from '../product/Select'
import { ConnectionFields } from './ConnectionFields'
import { draftFrom, PLACEMENT_OPTIONS, toInput } from './data-source-draft'
import { LabelsEditor } from './LabelsEditor'
import { hasInvalidPoolFields, PoolFields } from './PoolFields'

/**
 * Declares a data source, or corrects one that already exists.
 *
 * `existing` is `null` for a new declaration and the row being corrected
 * otherwise. Its id becomes read-only: an id names the row (ADR 0023 part
 * 1), and there is no rename here, only a correction of everything else
 * about it.
 *
 * The discriminator column only appears when `placement` is `shared` --
 * ADR 0006 makes it the only isolation a shared source may serve, and the
 * control plane refuses one on any other placement, so the form never asks
 * for a value it already knows will be refused.
 */
export function DataSourceForm({
  existing,
  saving,
  onSubmit,
  onCancel,
}: {
  existing: DataSource | null
  saving: boolean
  onSubmit: (id: string, input: DataSourceInput) => void
  onCancel: () => void
}) {
  const [id, setId] = useState(existing?.id ?? '')
  const [draft, setDraft] = useState(() => draftFrom(existing))

  return (
    <form
      className="panel panel-body"
      onSubmit={(event) => {
        event.preventDefault()
        // The three number fields show their own errors beside them
        // (`PoolFields`); this is what keeps an empty or non-numeric one
        // from ever reaching `toInput`, which would otherwise send it as
        // `0` or `null`.
        if (hasInvalidPoolFields(draft)) {
          return
        }
        onSubmit(id, toInput(draft))
      }}
    >
      <fieldset disabled={saving}>
        <div className="form-grid">
          <Field label="Data source ID" required readOnly={existing !== null} value={id} onChange={setId} />
          <Field
            label="Connector"
            required
            value={draft.connector}
            onChange={(connector) => {
              setDraft({ ...draft, connector })
            }}
          />
          <Select
            label="Placement"
            required
            value={draft.placement}
            options={PLACEMENT_OPTIONS}
            onChange={(value) => {
              const placement = PLACEMENT_OPTIONS.find((option) => option.value === value)?.value
              if (placement) {
                setDraft({ ...draft, placement })
              }
            }}
          />
          <Field
            label="Region"
            required
            value={draft.region}
            onChange={(region) => {
              setDraft({ ...draft, region })
            }}
          />
          <Field
            label="Jurisdiction"
            value={draft.jurisdiction}
            onChange={(jurisdiction) => {
              setDraft({ ...draft, jurisdiction })
            }}
          />
          {draft.placement === 'shared' && (
            <Field
              label="Discriminator column"
              required
              value={draft.discriminatorColumn}
              onChange={(discriminatorColumn) => {
                setDraft({ ...draft, discriminatorColumn })
              }}
              hint="The column every collection on this data source carries."
            />
          )}
        </div>

        <ConnectionFields
          kind={draft.connectionKind}
          value={draft.connectionValue}
          onChange={(connectionKind, connectionValue) => {
            setDraft({ ...draft, connectionKind, connectionValue })
          }}
        />

        <PoolFields
          draft={draft}
          onChange={(change) => {
            setDraft({ ...draft, ...change })
          }}
        />

        <Check
          label="Writable"
          value={draft.writable}
          onChange={(writable) => {
            setDraft({ ...draft, writable })
          }}
        />
        <Check
          label="Accepts new tenants"
          value={draft.acceptsNewTenants}
          onChange={(acceptsNewTenants) => {
            setDraft({ ...draft, acceptsNewTenants })
          }}
        />

        <LabelsEditor
          value={draft.labels}
          onChange={(labels) => {
            setDraft({ ...draft, labels })
          }}
        />

        <div className="form-actions">
          <button className="primary-button" type="submit">
            {existing ? 'Save data source' : 'Declare data source'}
          </button>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </fieldset>
    </form>
  )
}
