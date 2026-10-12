/**
 * The readers rows (G7): every answer says why, `applied` is shown as a
 * sample rather than a promise, `noReaders` is never withdrawal, and an
 * update reads complete only when the API says so.
 */
import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import type { Readers } from '../api/publication-types'
import { ReaderRows } from './ReaderRows'

const SHA = 'ab'.repeat(32)

function readers(overrides: Partial<Readers> = {}): Readers {
  return {
    state: 'applied',
    reasons: ['in this sample every replica that could serve reports generation 4 with the stamped record'],
    expected: { generation: 4, recordSha256: SHA },
    observedAtUnixSeconds: 1_700_000_000,
    desiredReplicas: 2,
    members: [
      {
        pod: 'fabric-api-a',
        replicaSet: 'fabric-api-7d9',
        finding: 'matches',
        detail: null,
        served: { generation: 4, recordSha256: SHA, origin: 'published' },
      },
    ],
    ...overrides,
  }
}

function fact(label: string): Element | null {
  return screen.getByText(label).nextElementSibling
}

describe('readers', () => {
  it('shows an applied sample with the record it was held to and each replica beside its replica set', () => {
    render(<ReaderRows readers={readers()} updateComplete />)

    expect(fact('Readers')).toHaveTextContent(/^●Applied — in this sample every replica/)
    expect(fact('Expected record')).toHaveTextContent(/^generation 4 \(abababababab\)$/)
    expect(fact('Sampled')).toHaveTextContent(/2 replicas asked for$/)
    expect(fact('Replicas')).toHaveTextContent(/^fabric-api-a \(fabric-api-7d9\) matches, generation 4 \(abababababab\)$/)
    expect(fact('Update complete')).toHaveTextContent(/^●Yes$/)
  })

  it('says why a sample is not applied, naming each replica and its failure', () => {
    render(
      <ReaderRows
        readers={readers({
          state: 'unobserved',
          reasons: ['fabric-api-b (fabric-api-6c1) could not be reached', 'fabric-api-a (fabric-api-7d9) serves an older generation'],
          members: [
            {
              pod: 'fabric-api-a',
              replicaSet: 'fabric-api-7d9',
              finding: 'behind',
              detail: null,
              served: { generation: 3, recordSha256: 'cd'.repeat(32), origin: 'published' },
            },
            { pod: 'fabric-api-b', replicaSet: 'fabric-api-6c1', finding: 'failed', detail: 'could not be reached', served: null },
          ],
        })}
        updateComplete={false}
      />,
    )

    expect(fact('Readers')).toHaveTextContent(/^●Unobserved — fabric-api-b \(fabric-api-6c1\) could not be reached; fabric-api-a/)
    expect(fact('Replicas')).toHaveTextContent(
      /^fabric-api-a \(fabric-api-7d9\) behind, generation 3 \(cdcdcdcdcdcd\); fabric-api-b \(fabric-api-6c1\) no answer: could not be reached$/,
    )
    expect(fact('Update complete')).toHaveTextContent(/^●No$/)
  })

  it('shows zero replicas as no readers, never as applied or withdrawn', () => {
    render(
      <ReaderRows
        readers={readers({
          state: 'noReaders',
          reasons: ['replicas are 0, so no replica serves'],
          desiredReplicas: 0,
          members: [],
        })}
        updateComplete={false}
      />,
    )

    expect(fact('Readers')).toHaveTextContent(/^●No readers — replicas are 0, so no replica serves$/)
    expect(fact('Replicas')).toHaveTextContent(/^None$/)
    expect(screen.queryByText(/withdrawn/i)).not.toBeInTheDocument()
    expect(screen.queryByText(/^Applied$/)).not.toBeInTheDocument()
  })

  it('says not sampled and shows a dash when nothing has been sampled or stamped', () => {
    render(
      <ReaderRows
        readers={readers({ state: 'unobserved', reasons: ['no reader observation is configured'], expected: null, observedAtUnixSeconds: null, desiredReplicas: null, members: [] })}
        updateComplete={false}
      />,
    )

    expect(fact('Expected record')).toHaveTextContent(/^—$/)
    expect(fact('Sampled')).toHaveTextContent(/^Not sampled$/)
  })
})
