// Dropdown, popover and tooltip structure adapted from shadcn/ui. See docs/ui-review/THIRD_PARTY.md.
import type { ComponentProps, ReactElement, ReactNode } from 'react'
import * as Dropdown from '@radix-ui/react-dropdown-menu'
import * as Popover from '@radix-ui/react-popover'
import * as Tooltip from '@radix-ui/react-tooltip'

export const TooltipProvider = Tooltip.Provider
export const ToolPopoverClose = Popover.Close

export function HelpTip({ label, side = 'top', children }: {
  label: string; side?: ComponentProps<typeof Tooltip.Content>['side']; children: ReactElement
}) {
  return <Tooltip.Root>
    <Tooltip.Trigger asChild>{children}</Tooltip.Trigger>
    <Tooltip.Portal>
      <Tooltip.Content className="ftw-tooltip" side={side} sideOffset={6}>{label}</Tooltip.Content>
    </Tooltip.Portal>
  </Tooltip.Root>
}

export function IconButton({ label, className = '', children, ...props }: ComponentProps<'button'> & { label: string }) {
  return <HelpTip label={label}>
    <button type="button" aria-label={label} className={`icon-control ${className}`} {...props}>{children}</button>
  </HelpTip>
}

export function ActionMenu({ trigger, children }: { trigger: ReactElement; children: ReactNode }) {
  return <Dropdown.Root modal={false}>
    <Dropdown.Trigger asChild>{trigger}</Dropdown.Trigger>
    <Dropdown.Portal>
      <Dropdown.Content align="end" sideOffset={6} className="ftw-popup ftw-menu">{children}</Dropdown.Content>
    </Dropdown.Portal>
  </Dropdown.Root>
}

export function MenuAction({ children, ...props }: ComponentProps<typeof Dropdown.Item>) {
  return <Dropdown.Item className="ftw-menu-item" {...props}>{children}</Dropdown.Item>
}

export function ToolPopover({ trigger, label, className = '', children }: {
  trigger: ReactElement; label: string; className?: string; children: ReactNode
}) {
  return <Popover.Root>
    <Popover.Trigger asChild>{trigger}</Popover.Trigger>
    <Popover.Portal>
      <Popover.Content align="end" sideOffset={6} aria-label={label} className={`ftw-popup ftw-popover ${className}`}>
        {children}
      </Popover.Content>
    </Popover.Portal>
  </Popover.Root>
}
