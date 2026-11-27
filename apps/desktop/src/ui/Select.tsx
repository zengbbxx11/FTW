// FTW styling on Radix Select / Popover and cmdk. See THIRD_PARTY_UI.txt.
import { Children, Fragment, forwardRef, isValidElement, useId, useRef, useState, type ChangeEventHandler, type ComponentPropsWithoutRef, type ReactNode } from 'react'
import * as RadixSelect from '@radix-ui/react-select'
import * as Popover from '@radix-ui/react-popover'
import { Command } from 'cmdk'
import { Check, ChevronDown, ChevronUp, Search } from 'lucide-react'
import './select.css'

type Choice = { value: string; label: string; disabled: boolean; group?: string }
type Props = Omit<ComponentPropsWithoutRef<'button'>, 'value' | 'defaultValue' | 'onChange' | 'children'> & {
  children: ReactNode; value?: string | number; defaultValue?: string | number; required?: boolean
  onChange?: ChangeEventHandler<HTMLSelectElement>; searchable?: boolean
}
function textOf(node: ReactNode): string {
  return Children.toArray(node).map(child => isValidElement<{ children?: ReactNode }>(child) ? textOf(child.props.children) : String(child ?? '')).join('')
}
function choicesOf(children: ReactNode, group?: string): Choice[] {
  const choices: Choice[] = []
  Children.forEach(children, child => {
    if (!isValidElement<{ children?: ReactNode; value?: string | number; disabled?: boolean; hidden?: boolean; label?: string }>(child)) return
    if (child.type === Fragment || child.type === 'optgroup') choices.push(...choicesOf(child.props.children, child.props.label ?? group))
    else if (child.type === 'option' && !child.props.hidden) choices.push({ value: String(child.props.value ?? textOf(child.props.children)), label: textOf(child.props.children), disabled: Boolean(child.props.disabled), group })
  })
  return choices
}

export const Select = forwardRef<HTMLButtonElement, Props>(function Select({ children, value, defaultValue, onChange, searchable, className = '', disabled, required, name, autoFocus, id, ...props }, forwardedRef) {
  const choices = choicesOf(children)
  const [internal, setInternal] = useState(String(defaultValue ?? choices[0]?.value ?? ''))
  const current = value === undefined ? internal : String(value)
  const [open, setOpen] = useState(false)
  const [container, setContainer] = useState<HTMLElement | undefined>()
  const trigger = useRef<HTMLButtonElement>(null)
  const native = useRef<HTMLSelectElement>(null)
  const empty = useId()
  const selected = choices.find(choice => choice.value === current)
  const search = searchable ?? choices.length > 10
  function toggle(next: boolean) {
    if (next) setContainer(trigger.current?.closest<HTMLElement>('dialog[open], [popover]') ?? undefined)
    setOpen(next)
  }
  function choose(next: string) {
    const control = native.current!
    control.value = next
    control.dispatchEvent(new Event('change', { bubbles: true }))
    toggle(false)
  }
  function assign(node: HTMLButtonElement | null) {
    trigger.current = node
    if (typeof forwardedRef === 'function') forwardedRef(node)
    else if (forwardedRef) forwardedRef.current = node
  }
  const buttonProps = { ...props, id, disabled, autoFocus, ref: assign, className: `ftw-select-trigger ${className}`,
    'aria-required': required, 'data-empty': !current }
  const mirror = <select hidden aria-hidden="true" tabIndex={-1} ref={native} value={current} disabled={disabled} required={required} name={name}
    onInvalid={event => { event.preventDefault(); trigger.current?.focus(); toggle(true) }}
    onChange={event => { setInternal(event.target.value); onChange?.(event) }}>{children}</select>
  const content = <><span className="ftw-select-value">{(selected?.label ?? current) || '请选择'}</span><ChevronDown className="ftw-select-chevron" aria-hidden="true" /></>
  if (search) return <span className="ftw-select-control">
    <Popover.Root open={open} onOpenChange={toggle}>
      <Popover.Trigger asChild><button {...buttonProps} type="button" role="combobox" aria-expanded={open} onKeyDown={event => { if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); toggle(true) } }}>{content}</button></Popover.Trigger>
      <Popover.Portal container={container}><Popover.Content className="ftw-select-popup ftw-combobox" align="start" sideOffset={5} collisionPadding={10}>
        <Command label={props['aria-label'] ?? '选择选项'}>
          <div className="ftw-select-search"><Search aria-hidden="true" /><Command.Input placeholder="搜索选项…" autoFocus /></div>
          <Command.List><Command.Empty>没有匹配的选项</Command.Empty>
            {choices.map(choice => <Command.Item key={choice.value} value={choice.value || empty} keywords={[choice.label, choice.group ?? '']} disabled={choice.disabled} onSelect={() => choose(choice.value)}>
              <span>{choice.label}</span>{current === choice.value && <Check aria-hidden="true" />}
            </Command.Item>)}
          </Command.List>
        </Command>
      </Popover.Content></Popover.Portal>
    </Popover.Root>
    {mirror}
  </span>
  return <span className="ftw-select-control">
    <RadixSelect.Root value={current || empty} onValueChange={next => choose(next === empty ? '' : next)} open={open} onOpenChange={toggle} disabled={disabled}>
      <RadixSelect.Trigger {...buttonProps}>{content}</RadixSelect.Trigger>
      <RadixSelect.Portal container={container}><RadixSelect.Content className="ftw-select-popup" position="popper" align="start" sideOffset={5} collisionPadding={10}>
        <RadixSelect.ScrollUpButton className="ftw-select-scroll"><ChevronUp aria-hidden="true" /></RadixSelect.ScrollUpButton>
        <RadixSelect.Viewport>{choices.map(choice => <RadixSelect.Item className="ftw-select-item" key={choice.value} value={choice.value || empty} disabled={choice.disabled}>
          <RadixSelect.ItemText>{choice.label}</RadixSelect.ItemText><RadixSelect.ItemIndicator><Check aria-hidden="true" /></RadixSelect.ItemIndicator>
        </RadixSelect.Item>)}</RadixSelect.Viewport>
        <RadixSelect.ScrollDownButton className="ftw-select-scroll"><ChevronDown aria-hidden="true" /></RadixSelect.ScrollDownButton>
      </RadixSelect.Content></RadixSelect.Portal>
    </RadixSelect.Root>
    {mirror}
  </span>
})
