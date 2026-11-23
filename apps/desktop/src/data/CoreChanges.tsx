import { useEffect, useRef } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { invoke } from '@tauri-apps/api/core'

import { changedQueryKeys, type Versions } from './changeKeys'

export function CoreChanges() {
  const cache = useQueryClient()
  const previous = useRef<Versions | undefined>(undefined)
  const { data } = useQuery({
    queryKey: ['core-changes'],
    queryFn: () => invoke<Versions>('core_changes'),
    refetchOnWindowFocus: 'always',
    refetchInterval: query => query.state.status === 'error' ? 8000 : 2000,
  })

  useEffect(() => {
    if (!data) return
    const last = previous.current
    previous.current = data
    const keys = changedQueryKeys(last, data)
    for (const key of keys) void cache.invalidateQueries({ queryKey: [key] })
  }, [cache, data])
  return null
}
