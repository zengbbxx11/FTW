import { invoke } from '@tauri-apps/api/core'
import { useMutation } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import { mailError } from './api'

export function AttachmentActions({ source, id, index, filename }: {
  source: 'inbox' | 'outbound'; id: string; index: number; filename: string
}) {
  const save = useMutation({
    mutationFn: () => invoke<boolean>(source === 'inbox' ? 'save_inbox_attachment' : 'save_outbound_attachment', { id, index, filename }),
  })
  return <div>
    <button type="button" className="button" disabled={save.isPending} onClick={() => save.mutate()}>
      <Download aria-hidden="true" />另存附件
    </button>
    {save.data && <small role="status">附件已保存</small>}
    {save.isError && <p className="alert error" role="alert">{mailError(save.error)}</p>}
  </div>
}
