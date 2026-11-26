import { useState, useSyncExternalStore, type ReactNode } from 'react'
import { PanelLeftClose, PanelLeftOpen, Monitor, type LucideIcon } from 'lucide-react'
import { HelpTip, IconButton, TooltipProvider } from './ui/WorkbenchUI'

type NavigationItem<Kind> = { kind: Kind; label: string; active: boolean; icon: LucideIcon }
const sidebarPreference = 'ftw.navigation.collapsed'
const compactWidth = window.matchMedia('(max-width: 1100px)')

function subscribeToWidth(onChange: () => void) {
  compactWidth.addEventListener('change', onChange)
  return () => compactWidth.removeEventListener('change', onChange)
}

function readSidebarPreference() {
  const saved = localStorage.getItem(sidebarPreference)
  return saved === null ? null : saved === 'true'
}

// Header/content/footer composition adapted from Shadcn Admin's AppSidebar.
export function AppShell<Kind extends string>({ navigation, navigate, page, children }: {
  navigation: NavigationItem<Kind>[]; navigate: (kind: Kind) => void; page: string; children: ReactNode
}) {
  const [collapsed, setCollapsed] = useState(readSidebarPreference)
  const narrow = useSyncExternalStore(subscribeToWidth, () => compactWidth.matches)
  const isCompact = collapsed ?? narrow
  function toggle() {
    localStorage.setItem(sidebarPreference, String(!isCompact))
    setCollapsed(!isCompact)
  }
  return <TooltipProvider delayDuration={350}>
    <div className="app-shell" data-sidebar={isCompact ? 'collapsed' : 'expanded'}>
      <a className="skip-link" href="#workbench">跳到工作区</a>
      <aside className="app-header" aria-label="FTW">
        <div className="brand"><div className="brand-mark" aria-hidden="true">F</div>
          <div className="brand-name">FTW <span>外贸智能工作台</span></div>
        </div>
        <nav className="app-nav" id="workbench-navigation" aria-label="工作台导航">
          {navigation.map(({ kind, label, active, icon: Icon }) => <HelpTip key={kind} label={label} side="right">
            <button type="button" onClick={() => navigate(kind)} aria-label={label} aria-current={active ? 'page' : undefined}
              aria-keyshortcuts={kind === 'search' ? 'Control+K' : undefined}>
              <Icon aria-hidden="true" strokeWidth={1.75} /><span>{label}</span>
            </button>
          </HelpTip>)}
        </nav>
        <div className="sidebar-footer">
          <div className="sidebar-profile"><span className="sidebar-profile-icon"><Monitor aria-hidden="true" /></span>
            <span>本地工作空间</span></div>
          <IconButton className="sidebar-toggle" label={isCompact ? '展开侧栏' : '折叠侧栏'}
            aria-controls="workbench-navigation" aria-expanded={!isCompact} onClick={toggle}>
            <PanelLeftClose aria-hidden="true" className="sidebar-close-icon" />
            <PanelLeftOpen aria-hidden="true" className="sidebar-open-icon" />
          </IconButton>
        </div>
      </aside>
      <main className="workspace" id="workbench" data-page={page} tabIndex={-1}>{children}</main>
    </div>
  </TooltipProvider>
}
