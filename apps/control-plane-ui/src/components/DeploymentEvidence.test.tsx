import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { DeploymentEvidence } from './DeploymentEvidence'

describe('deployment evidence', () => {
  it('distinguishes missing configuration from an observation failure', () => {
    const { rerender } = render(<DeploymentEvidence />)
    expect(screen.getByText(/not configured/)).toBeInTheDocument()
    rerender(<DeploymentEvidence observation={{ observedAtUnixSeconds: 1700000000,
      version: null, health: 'unavailable', detail: 'Deployment observation is not permitted.', workloads: [] }} />)
    expect(screen.getByText('Unavailable')).toBeInTheDocument()
    expect(screen.getByText(/not permitted/)).toBeInTheDocument()
    expect(screen.queryByText(/not configured/)).not.toBeInTheDocument()
  })
  it('shows mixed releases and a stopped runtime without claiming convergence', () => {
    render(<DeploymentEvidence observation={{ observedAtUnixSeconds: 1700000000,
      version: null, health: 'progressing', detail: null, workloads: [
        { name: 'api', health: 'progressing', versions: ['v1.0.0', 'v1.0.1'], desiredReplicas: 1, readyReplicas: 1, detail: null },
        { name: 'runtime', health: 'stopped', versions: [], desiredReplicas: 0, readyReplicas: 0, detail: null },
      ] }} />)
    expect(screen.getByText(/v1.0.0, v1.0.1/)).toBeInTheDocument()
    expect(screen.getByText(/Stopped/)).toBeInTheDocument()
    expect(screen.queryByText('Healthy')).not.toBeInTheDocument()
  })
})
