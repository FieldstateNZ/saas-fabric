import type { ProjectedClientField, ProjectionCheck } from '../api/types'

/** Each field, as an operator would describe the difference. */
const FIELD: Record<ProjectedClientField, string> = {
  missing: 'is no longer in the stored configuration',
  protocol: 'has a different protocol',
  pkce: 'has a different PKCE method',
  redirectStrategy: 'has a different callback strategy',
  redirectUris: 'has a different set of callbacks',
}

/**
 * Says when a client's stored projected identity clients are not what its
 * product configuration projects (D01-19b), before anyone tries a save.
 *
 * It reports the stored configuration as it is and says nothing about who
 * changed it, because the console cannot know. Nothing is shown when the
 * projection is consistent.
 */
export function ProjectionNotice({ projection }: { projection: ProjectionCheck }) {
  if (projection.state === 'consistent') {
    return null
  }

  return (
    <div className="projection-notice" role="status">
      {projection.state === 'diverged' ? (
        <>
          <p>
            The stored identity configuration no longer matches what this client&apos;s applications project.
            Product saves and re-assertion are refused, and the affected applications are not applied, until
            it matches again: restore it in the configuration repository, or edit it back to the projection.
          </p>
          <ul>
            {projection.divergences.map((divergence) => (
              <li key={divergence.client}>
                <code>{divergence.client}</code> {FIELD[divergence.field]}
              </li>
            ))}
          </ul>
        </>
      ) : (
        <>
          <p>
            What this client&apos;s applications project cannot be worked out from its stored configuration, so
            these identity clients cannot be checked against it and are not applied:
          </p>
          <ul>
            {projection.unverified.map((client) => (
              <li key={client}>
                <code>{client}</code>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}
