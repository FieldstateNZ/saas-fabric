import { ConvergeButton } from '../components/ConvergeButton'
import type { Inventory } from './useInventory'
import { Activity } from './Activity'
import { PageHeader } from './primitives'

export function Reconciliation({ inventory, connected }: { inventory: Inventory; connected: boolean }) {
  return <><PageHeader title="Reconciliation" description="Compare desired identity configuration with what has been applied."
    actions={<button onClick={inventory.refresh} disabled={inventory.loading}>{inventory.loading ? 'Refreshing…' : 'Refresh observations'}</button>} />
    <div className="reconciliation-intro"><div><h2>Bring client identities into sync</h2>
      <p>A saved change remains pending until reconciliation confirms it has taken effect.</p></div>
      {connected && <ConvergeButton onComplete={inventory.refresh} />}</div>
    <Activity inventory={inventory} />
    <p className="support-note">These are the latest identity observations. A full run history is not available yet.</p>
  </>
}
