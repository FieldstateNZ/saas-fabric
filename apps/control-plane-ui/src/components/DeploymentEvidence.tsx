import type { DeploymentObservation } from '../api/types'

/** A timestamp makes this a sample, not a promise of ongoing health. */
export function DeploymentEvidence({ observation }: { observation?: DeploymentObservation | undefined }) {
  if (!observation) return <p>Deployment observation is not configured.</p>
  return (
    <div className="platform__evidence">
      <dl className="platform__rows">
        <dt>Deployment</dt><dd>{label(observation.health)}</dd>
        <dt>Observed</dt>
        <dd>{new Date(observation.observedAtUnixSeconds * 1000).toLocaleString()}</dd>
      </dl>
      {observation.detail && <p>{observation.detail}</p>}
      <ul>
        {observation.workloads.map((workload) => (
          <li key={workload.name}>
            <strong>{workload.name}</strong> — {label(workload.health)}
            {workload.desiredReplicas !== null && ` · ${workload.readyReplicas}/${workload.desiredReplicas} ready`}
            {workload.health !== 'stopped' && workload.versions.length > 0 && ` · ${workload.versions.join(', ')}`}
            {workload.detail && <p>{workload.detail}</p>}
          </li>
        ))}
      </ul>
    </div>
  )
}

function label(health: DeploymentObservation['health']): string {
  return { healthy: 'Healthy', progressing: 'Rollout in progress', degraded: 'Degraded',
    stopped: 'Stopped', unavailable: 'Unavailable' }[health]
}
