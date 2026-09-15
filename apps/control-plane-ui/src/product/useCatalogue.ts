import { useCallback, useEffect, useState } from 'react'
import { changeCatalogue, getCatalogue } from '../api/catalogue'
import type { CatalogueCommand, StoredCatalogue } from '../api/catalogue-types'
import { describe } from '../hooks/useClients'
export interface CatalogueState { value: StoredCatalogue | null; error: string | null; loading: boolean; saving: boolean; refresh: () => void; save: (command: CatalogueCommand) => Promise<boolean> }
export function useCatalogue(): CatalogueState {
  const [value, setValue] = useState<StoredCatalogue | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [generation, setGeneration] = useState(0)
  const refresh = useCallback(() => { setGeneration((n) => n + 1) }, [])
  useEffect(() => {
    let active = true
    setLoading(true)
    void getCatalogue().then((next) => { if (active) { setValue(next); setError(null) } }, (error: unknown) => { if (active) setError(describe(error)) }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [generation])
  const save = async (command: CatalogueCommand) => {
    if (!value || saving) return false
    setSaving(true); setError(null)
    try { setValue(await changeCatalogue(command, value.revision)); return true }
    catch (error: unknown) { setError(describe(error)); return false }
    finally { setSaving(false) }
  }
  return { value, loading, error, saving, refresh, save }
}
