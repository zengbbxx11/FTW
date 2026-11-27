import {
  Archive, ArrowDown, ArrowDownUp, ArrowUp, Building2, CalendarDays, Check, ChevronDown,
  ChevronLeft, ChevronRight, ChevronUp, CircleCheck, ClipboardList, Clock, ContactRound, Copy, Ellipsis,
  Filter, Globe, GripVertical, History, Kanban, ListFilter, Mail, MessageSquarePlus, Package, Pencil,
  Plus, RotateCcw, Save, Search, Send, ShieldCheck, SlidersHorizontal, Star, Table2,
  TrendingUp, Trash2, Upload, UsersRound, X,
} from 'lucide-react'

const icons = {
  delete: Trash2,
  users: UsersRound, grid: Table2, board: Kanban, star: Star, box: Package, growth: TrendingUp,
  checkCircle: CircleCheck, search: Search, filter: Filter, settings: SlidersHorizontal,
  plus: Plus, check: Check, edit: Pencil, arrow: ChevronRight, back: ChevronLeft, mail: Mail,
  contact: ContactRound, note: ClipboardList, clock: Clock, sort: ArrowDownUp, down: ArrowDown,
  up: ArrowUp, upload: Upload, shield: ShieldCheck, save: Save, close: X, more: Ellipsis,
  copy: Copy, send: Send, activity: History, addActivity: MessageSquarePlus, building: Building2,
  globe: Globe, calendar: CalendarDays, archive: Archive, restore: RotateCcw,
  grip: GripVertical, expand: ChevronDown, collapse: ChevronUp, sliders: ListFilter,
}

export function CustomerIcon({ name }: { name: keyof typeof icons }) {
  const Icon = icons[name]
  return <Icon className="customer-icon" aria-hidden="true" strokeWidth={1.75} />
}
