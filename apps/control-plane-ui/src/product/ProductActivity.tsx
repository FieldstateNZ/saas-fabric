import { useEffect, useState } from 'react'
import { getActivity } from '../api/catalogue'
import type { ProductActivity } from '../api/catalogue-types'
import { Panel } from '../console/primitives'
import { describe } from '../hooks/useClients'
export function ActivityTable({ activity }: { activity: ProductActivity[] }) {
  return <Panel title="Activity"><div className="table-wrap"><table className="fabric-table"><thead><tr><th>When</th><th>Resource</th><th>Action</th><th>Operator</th></tr></thead><tbody>{[...activity].sort((a, b) => b.at - a.at).map((action, index) => <tr key={index}>
    <td className="mono">{new Date(action.at * 1000).toLocaleString()}</td><td>{action.resource}</td><td>{action.action}</td><td>{action.operator}</td></tr>)}</tbody></table>{activity.length === 0 && <p className="panel-body">No recorded changes yet.</p>}</div></Panel>
}
export function ProductActivityFeed({ generation = 0 }: { generation?: number }) {
  const [refresh, setRefresh] = useState(0)
  const [activity, setActivity] = useState<ProductActivity[]>([])
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    let active = true
    void getActivity().then((value) => { if (active) { setActivity(value.activity); setError(null) } }, (error: unknown) => { if (active) setError(describe(error)) })
    return () => { active = false }
  }, [generation, refresh])
  return <><button type="button" onClick={() => { setRefresh((n) => n + 1) }}>Refresh activity</button>{error && <p className="error" role="alert">{error}</p>}<ActivityTable activity={activity} /></>
}
