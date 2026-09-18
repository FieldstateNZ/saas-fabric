import { Field } from '../product/Field'
import type { DataSourceDraft } from './data-source-draft'

/** The three pool fields this component edits, named the way the draft holds them. */
type PoolField = 'maxConnections' | 'idleTimeoutSeconds' | 'acquireTimeoutSeconds'

const POOL_FIELDS: readonly PoolField[] = ['maxConnections', 'idleTimeoutSeconds', 'acquireTimeoutSeconds']

/**
 * Why a pool field's raw text cannot become the number `toInput` would send,
 * or `null` when it can.
 *
 * `Number('')` is `0`, not an error, and `Number('abc')` is `NaN` -- neither
 * refusal reaches the operator through `toInput` alone, so an empty or
 * non-numeric field would otherwise be sent as `0`, or, once `NaN` is
 * serialised, as `null`. Shown beside the field this exists for, and used by
 * {@link DataSourceForm} to refuse submitting while any of the three holds
 * one.
 */
export function poolFieldError(value: string): string | null {
  const trimmed = value.trim()
  return trimmed === '' || !Number.isFinite(Number(trimmed)) ? 'Enter a number.' : null
}

/** Whether any of the draft's three pool fields cannot be sent as a number. */
export function hasInvalidPoolFields(draft: Pick<DataSourceDraft, PoolField>): boolean {
  return POOL_FIELDS.some((field) => poolFieldError(draft[field]) !== null)
}

/**
 * Connection-pool limits, edited as three plain numbers.
 *
 * Pulled out of {@link DataSourceForm} on its own -- these three fields
 * carry no branching and no relationship to anything else in the draft, so
 * giving them their own file is what keeps the form itself readable as one
 * screen's worth of decisions rather than a wall of fields.
 */
export function PoolFields({
  draft,
  onChange,
}: {
  draft: Pick<DataSourceDraft, 'maxConnections' | 'idleTimeoutSeconds' | 'acquireTimeoutSeconds'>
  onChange: (draft: Partial<DataSourceDraft>) => void
}) {
  return (
    <div className="form-grid">
      <Field
        label="Max connections"
        type="number"
        required
        value={draft.maxConnections}
        error={poolFieldError(draft.maxConnections)}
        onChange={(maxConnections) => {
          onChange({ maxConnections })
        }}
      />
      <Field
        label="Idle timeout (seconds)"
        type="number"
        required
        value={draft.idleTimeoutSeconds}
        error={poolFieldError(draft.idleTimeoutSeconds)}
        onChange={(idleTimeoutSeconds) => {
          onChange({ idleTimeoutSeconds })
        }}
      />
      <Field
        label="Acquire timeout (seconds)"
        type="number"
        required
        value={draft.acquireTimeoutSeconds}
        error={poolFieldError(draft.acquireTimeoutSeconds)}
        onChange={(acquireTimeoutSeconds) => {
          onChange({ acquireTimeoutSeconds })
        }}
      />
    </div>
  )
}
