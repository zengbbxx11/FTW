export const fieldKinds = {
  text: '文本', number: '数字', date: '日期', single_select: '单选',
  multi_select: '多选', checkbox: '复选框', url: '网址',
} as const
export const fieldColors = {
  gray: '灰色', teal: '青色', blue: '蓝色', green: '绿色', amber: '橙色', purple: '紫色', rose: '玫红',
} as const
export type FieldKind = keyof typeof fieldKinds
export type FieldColor = keyof typeof fieldColors
export type FieldAccess = 'hidden' | 'read' | 'write'
export type FieldValue =
  | { type: 'text'; value: string }
  | { type: 'date'; value: string }
  | { type: 'url'; value: string }
  | { type: 'single_select'; value: string }
  | { type: 'multi_select'; value: string[] }
  | { type: 'number'; value: number }
  | { type: 'checkbox'; value: boolean }
export type FieldChoice = { id: string; label: string; color: FieldColor; archived: boolean }
export type CustomField = {
  id: string; revision: number; name: string; kind: FieldKind; color: FieldColor
  agent_access: FieldAccess; show_in_details: boolean; default_value: FieldValue | null
  archived: boolean; options: FieldChoice[]
}
export type FieldDefinition = Omit<CustomField, 'id' | 'revision' | 'archived'> & {
  id: string | null; expected_revision: number | null
}
export type FieldEntry = { field_id: string; value: FieldValue | null }
export type FieldValues = { revision: number; epoch: string; entries: FieldEntry[] }
export type FieldPatch = { contact_id: string; expected_revision: number; expected_epoch: string; entries: FieldEntry[] }
export function fieldEntries(values: Record<string, FieldValue | null>): FieldEntry[] {
  return Object.entries(values).map(([field_id, value]) => ({ field_id, value }))
}
