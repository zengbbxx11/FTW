export type Versions = { epoch: string; domains: Record<string, number> }

const keysByDomain: Record<string, string[]> = {
  contacts: ['contacts', 'contact', 'development-overview', 'contact-history', 'lead', 'sample-workspace', 'customer-files', 'audit', 'dashboard', 'data-overview', 'global-search', 'agent-runs', 'agent-run-mail', 'mail-analytics'],
  activity: ['contacts', 'contact', 'development-overview', 'activities', 'reply-updates', 'dashboard', 'sample-workspace', 'global-search'],
  leads: ['leads', 'lead', 'contact', 'global-search', 'dashboard'],
  mail: ['contacts', 'contact', 'development-overview', 'mail-drafts', 'mail-draft', 'mail-queue-summary', 'mail-accounts', 'mail-suppression', 'customer-files', 'inbox', 'inbox-record', 'inbox-thread', 'reply-updates', 'imap-accounts', 'dashboard', 'data-overview', 'global-search', 'agent-runs', 'agent-run-mail', 'mail-analytics'],
  mail_expiration: ['mail-drafts', 'mail-draft', 'dashboard'],
  configuration: ['crm-states', 'contacts', 'contact'],
  audit: ['audit', 'dashboard', 'agent-runs'],
  custom_fields: ['crm-fields', 'contact-fields', 'contacts', 'crm-views'],
  crm_views: ['crm-views'],
  mail_assets: ['mail-assets', 'inbox-body', 'global-search'],
  mail_templates: ['mail-templates', 'mail-template-preview'],
  customer_documents: ['customer-files', 'global-search'],
  mail_agent_policy: ['agent-mail-settings'],
}


export function changedQueryKeys(last: Versions | undefined, next: Versions): Set<string> {
  const keys = new Set<string>()
  for (const [domain, names] of Object.entries(keysByDomain)) {
    if (!last || next.epoch !== last.epoch || next.domains[domain] !== last.domains[domain]) {
      names.forEach(name => keys.add(name))
    }
  }
  if (last && next.epoch !== last.epoch) keys.add('backups')
  return keys
}
