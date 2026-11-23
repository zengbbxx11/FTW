import { useId, useRef, type ReactNode } from 'react'
import { errorMessage, PAGE_SIZE, type Duplicate } from './api'

export function Field({ label, children }: { label: string; children: ReactNode }) {
  return <label className="field"><span>{label}</span>{children}</label>
}

export function Problem({ error }: { error: unknown }) {
  return <p className="alert error" role="alert">{errorMessage(error)}</p>
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="empty">{children}</div>
}

export function Pagination({ offset, more, change, pageSize = PAGE_SIZE }: { offset: number; more: boolean; change: (page: number) => void; pageSize?: number }) {
  if (!offset && !more) return null
  return (
    <div className="pagination" aria-label="分页">
      <button type="button" className="button" disabled={offset === 0} onClick={() => change(Math.max(0, offset - pageSize))}>上一页</button>
      <span>第 {Math.floor(offset / pageSize) + 1} 页</span>
      <button type="button" className="button" disabled={!more} onClick={() => change(offset + pageSize)}>下一页</button>
    </div>
  )
}

const reasons: Record<string, string> = {
  email: '邮箱相同',
  domain: '网站域名相同',
  company_country: '公司及国家或地区相同',
}

export function Duplicates({ matches, children }: { matches: Duplicate[]; children?: ReactNode }) {
  if (matches.length === 0) return null
  return (
    <div className="alert warning" role="status">
      <strong>发现可能重复的记录</strong>
      <ul>{matches.map(match => (
        <li key={match.entity + match.id}>
          {match.display_name} · {match.entity === 'contact' ? '正式客户' : '候选客户'} · {reasons[match.reason] ?? match.reason}
        </li>
      ))}</ul>
      {children}
    </div>
  )
}

export function ConfirmButton({ children, label, onConfirm, disabled }: {
  children: ReactNode; label: string; onConfirm: () => void; disabled?: boolean
}) {
  const dialog = useRef<HTMLDialogElement>(null)
  const id = useId()
  return <>
    <button className="button" type="button" disabled={disabled}
      onClick={() => dialog.current?.showModal()}>
      {children}
    </button>
    <dialog ref={dialog} className="confirmation" aria-labelledby={id}>
      <p id={id}>{label}</p>
      <div className="form-footer">
        <button className="button" type="button" autoFocus onClick={() => dialog.current?.close()}>取消</button>
        <button className="button primary" type="button" onClick={() => { dialog.current?.close(); onConfirm() }}>确认</button>
      </div>
    </dialog>
  </>
}
