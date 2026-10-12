import { useState } from 'react'

/**
 * The explicit, audited way back from a `handEdited` identity (D01-19b).
 *
 * An ordinary reconciliation leaves a hand edit SaaS Fabric does not support
 * in place rather than fighting it on every pass. If the edit should not
 * stand, an operator either undoes it in the identity provider or asks for
 * this, which writes the declared configuration over it as them and records
 * that they did. Two steps, because it overwrites what somebody chose.
 */
export function ReassertProjection({ busy, onReassert }: { busy: boolean; onReassert: () => Promise<void> }) {
  const [confirming, setConfirming] = useState(false)

  if (!confirming) {
    return (
      <div className="reassert">
        <button
          type="button"
          disabled={busy}
          onClick={() => {
            setConfirming(true)
          }}
        >
          Re-assert configuration
        </button>
      </div>
    )
  }

  return (
    <div className="reassert">
      <p className="reassert__warning">
        This overwrites the hand edits listed above with the configuration SaaS Fabric declares, and is
        recorded in the audit trail.
      </p>
      <button
        type="button"
        disabled={busy}
        onClick={() => {
          void onReassert().finally(() => {
            setConfirming(false)
          })
        }}
      >
        {busy ? 'Re-asserting...' : 'Overwrite hand edits'}
      </button>
      <button
        type="button"
        disabled={busy}
        onClick={() => {
          setConfirming(false)
        }}
      >
        Cancel
      </button>
    </div>
  )
}
