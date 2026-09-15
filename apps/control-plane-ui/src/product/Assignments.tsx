import { useState } from 'react'

import type { ApplicationRelease, AssignmentRequest, ProductApplication } from '../api/catalogue-types'
import { Check } from './Check'
import { ConfigurationInputs } from './ConfigurationInputs'
import { Select } from './Select'

/**
 * Which published applications a client is assigned, and their plan and
 * configuration.
 *
 * # Only published releases are offered
 *
 * `available` drops every application with no release: a draft has nothing
 * immutable to point an assignment at, and publishing is exactly the act
 * that makes one exist. Assigning always targets `releases.at(-1)` — the
 * newest — and an operator moves a client to an older one afterward by
 * changing the version, never by this control choosing one. Every label
 * here reads from that release's own definition name, not the application's
 * unpublished draft name, which can already say something different.
 *
 * # `locked` refuses removal, and says why
 *
 * An application already assigned to the client being edited cannot be
 * unassigned from here — removal is refused until deprovisioning exists, so
 * the checkbox is disabled rather than merely reverted, and a note explains
 * the constraint instead of the control quietly ignoring the click.
 *
 * # Switching versions keeps what still applies
 *
 * A version change keeps the current plan when the new release still has a
 * plan with that id, and keeps every configuration value whose key is still
 * one of the new release's fields — dropping only what no longer exists
 * there. When either was reset, {@link resetNotice} says so, until whatever
 * the operator does next to this assignment: unchecking and re-checking it,
 * picking a plan by hand, or editing a configuration value. Any of those is
 * a deliberate change made after seeing the note, not still the version
 * switch it was about.
 *
 * This file sits in file-size-policy.md's 121-150 line band. `retarget` is
 * a pure function with exactly one caller, and the two together are one
 * concept — what happens to an assignment when its version changes — the
 * same way `ValuesEditor` keeps its own private `parseLimits` rather than
 * splitting a single-caller helper into a file of its own.
 */
export function Assignments({
  apps,
  value,
  onChange,
  locked = [],
}: {
  apps: readonly ProductApplication[]
  value: readonly AssignmentRequest[]
  onChange: (value: AssignmentRequest[]) => void
  locked?: readonly string[]
}) {
  const [resetNotice, setResetNotice] = useState<Record<string, boolean>>({})
  const available = apps.filter((app) => app.releases.length > 0)
  const change = (next: AssignmentRequest) => {
    onChange(value.map((item) => (item.applicationId === next.applicationId ? next : item)))
  }

  return (
    <div className="assignment-list">
      {available.length === 0 && (
        <p>
          No published applications yet. Publish an application with at least one plan to assign it.
        </p>
      )}
      {available.map((app) => {
        const assignment = value.find((item) => item.applicationId === app.id)
        const latest = app.releases.at(-1)
        const release = app.releases.find((r) => r.version === assignment?.version)
        const name = latest?.definition.name ?? app.draft.name
        const isLocked = locked.includes(app.id)

        return (
          <section className="assignment" key={app.id}>
            <Check
              label={name}
              value={Boolean(assignment)}
              disabled={isLocked}
              onChange={(checked) => {
                setResetNotice((prev) => ({ ...prev, [app.id]: false }))
                if (checked && latest?.definition.plans[0]) {
                  onChange([
                    ...value,
                    {
                      applicationId: app.id,
                      version: latest.version,
                      planId: latest.definition.plans[0].id,
                      configuration: {},
                    },
                  ])
                } else if (!isLocked) {
                  onChange(value.filter((item) => item.applicationId !== app.id))
                }
              }}
            />
            {assignment && release && (
              <>
                <div className="form-grid">
                  <Select
                    label={`${name} version`}
                    value={String(assignment.version)}
                    options={app.releases.map((r) => ({
                      value: String(r.version),
                      label: `Definition v${String(r.version)}`,
                    }))}
                    onChange={(version) => {
                      const selected = app.releases.find((r) => r.version === Number(version))
                      if (!selected) {
                        return
                      }
                      const { next, reset } = retarget(assignment, selected)
                      change(next)
                      setResetNotice((prev) => ({ ...prev, [app.id]: reset }))
                    }}
                  />
                  <Select
                    label={`${name} plan`}
                    value={assignment.planId}
                    options={release.definition.plans.map((p) => ({ value: p.id, label: p.name }))}
                    onChange={(planId) => {
                      change({ ...assignment, planId })
                      setResetNotice((prev) => ({ ...prev, [app.id]: false }))
                    }}
                  />
                </div>
                <ConfigurationInputs
                  fields={release.definition.fields}
                  values={assignment.configuration}
                  onChange={(configuration) => {
                    change({ ...assignment, configuration })
                    setResetNotice((prev) => ({ ...prev, [app.id]: false }))
                  }}
                />
                <p className="support-note">
                  Features:{' '}
                  {release.definition.plans
                    .find((plan) => plan.id === assignment.planId)
                    ?.features.join(', ') || 'Core components only'}
                </p>
                {resetNotice[app.id] && (
                  <p className="support-note">
                    Switching versions reset the plan or configuration values that no longer exist
                    in this release.
                  </p>
                )}
                {isLocked && (
                  <p className="support-note">
                    This application is assigned. Removal requires deprovisioning.
                  </p>
                )}
              </>
            )}
          </section>
        )
      })}
    </div>
  )
}

/**
 * Carries an assignment forward onto a different release: the same plan if
 * it still exists there, otherwise the release's first plan; the same
 * configuration values for every key still named by one of the release's
 * fields, and nothing else. `reset` is true when either of those actually
 * changed something, so the caller knows whether to tell the operator.
 */
function retarget(
  assignment: AssignmentRequest,
  release: ApplicationRelease,
): { next: AssignmentRequest; reset: boolean } {
  const plans = release.definition.plans
  const keptPlan = plans.find((plan) => plan.id === assignment.planId)
  const planId = keptPlan ? keptPlan.id : (plans[0]?.id ?? '')

  const fieldKeys = new Set(release.definition.fields.map((field) => field.key))
  const configuration: Record<string, string> = {}
  let droppedValue = false
  for (const [key, entry] of Object.entries(assignment.configuration)) {
    if (fieldKeys.has(key)) {
      configuration[key] = entry
    } else {
      droppedValue = true
    }
  }

  return {
    next: { ...assignment, version: release.version, planId, configuration },
    reset: !keptPlan || droppedValue,
  }
}
