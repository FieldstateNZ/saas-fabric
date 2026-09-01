import type { Platform, PlatformComponent, PlatformLastCheck } from '../api/types'
import { ComponentBrake } from './ComponentBrake'

/**
 * What an environment is asked to run.
 *
 * # Six lines, and no more
 *
 * There is no history, no release notes, no deployment status and no
 * third-party component here. Every one of those is a thing the platform
 * cannot yet answer honestly, and a console that showed them would be showing
 * a guess. The six lines below are the ones that are true.
 */
export function PlatformPanel({ platform }: { platform: Platform }) {
  return (
    <section className="platform">
      <h2 className="platform__heading">Platform / {platform.environment}</h2>

      {platform.components.map((component) => (
        <ComponentRows key={component.component} component={component} />
      ))}

      <dl className="platform__rows">
        <dt>Last check</dt>
        <dd>
          <LastCheck check={platform.lastCheck} />
        </dd>
      </dl>
    </section>
  )
}

/** One component, as six lines. */
function ComponentRows({ component }: { component: PlatformComponent }) {
  return (
    <article className="platform__component">
      <h3 className="platform__component-name">{component.component}</h3>

      {/* Two groups, because they answer different questions. The first is
          the three-state model: what this environment is asked to run, what it
          would move to, and what is actually serving. The second is about the
          decision: whether it may move, whether it needs to, and when anything
          last looked. */}
      <dl className="platform__rows">
        <dt>Desired</dt>
        <dd>{component.desired}</dd>

        {/* "Newer version", not "Available". This is the newest eligible
            version *newer than desired* — nothing here observed whether the
            desired version is still published, so calling it Available would
            render `—` about an environment running the newest preview there
            is, and say something false while doing it.

            A `Latest available` worth the name arrives with a versions view,
            where Fabric enumerates what exists rather than inferring it from
            what it declined to advance to. */}
        <dt>Newer version</dt>
        <dd>{component.newer ?? '—'}</dd>

        <dt>Running</dt>
        <dd>Unknown</dd>
      </dl>

      <dl className="platform__rows platform__rows--decision">
        <dt>Policy</dt>
        <dd>{policy(component)}</dd>

        {/* Some overlap with "Newer version", and it is the useful kind: that
            row answers "what would Fabric advance to", this one answers "does
            desired state need advancing". */}
        <dt>Desired state</dt>
        <dd>{component.desiredState === 'current' ? 'Current' : 'Update available'}</dd>
      </dl>

      {component.hold !== null && (
        <p className="platform__hold">
          Paused: {component.hold.reason}
          {component.hold.note !== null && ` — ${component.hold.note}`}
        </p>
      )}

      <ComponentBrake component={component} />

      {component.diagnostics.length > 0 && (
        <ul className="platform__diagnostics">
          {component.diagnostics.map((diagnostic) => (
            <li key={diagnostic.version}>
              {diagnostic.version} —{' '}
              {diagnostic.state === 'publishing' ? 'still publishing' : 'built more than once'}
            </li>
          ))}
        </ul>
      )}
    </article>
  )
}

/**
 * The policy, and whether it is paused.
 *
 * `Automatic — Paused` rather than a third policy value, because the operator
 * paused advancement and did not change what this environment should do.
 */
function policy(component: PlatformComponent): string {
  const name =
    component.policy === 'automatic'
      ? 'Automatic'
      : component.policy === 'manual'
        ? 'Manual'
        : 'Locked'

  return component.paused ? `${name} — Paused` : name
}

/**
 * When something last looked, and what it found.
 *
 * `Never` is the answer that matters most. Without it, an operator whose
 * published version has not appeared cannot tell whether nothing has checked,
 * something checked and found nothing, or something checked and failed — and
 * those send them three different places.
 */
function LastCheck({ check }: { check: PlatformLastCheck | null }) {
  if (check === null) {
    return <>Never</>
  }

  const at = new Date(check.atUnixSeconds * 1000).toLocaleTimeString()

  if (check.outcome === 'success') {
    return <>{at} — success</>
  }

  return (
    <span className="platform__failure">
      {at} — {check.detail ?? 'failed'}
    </span>
  )
}

/**
 * A deployment that manages no platform repository.
 *
 * Not an error, and deliberately not styled as one. It is a state an operator
 * can act on, and there is nothing here yet for them to act *with* — so it
 * says what is true and stops, rather than offering a settings flow that does
 * not exist.
 */
export function PlatformNotManaged() {
  return (
    <section className="platform">
      <h2 className="platform__heading">Platform</h2>
      <p className="empty">Platform Management is not connected for this deployment.</p>
    </section>
  )
}
