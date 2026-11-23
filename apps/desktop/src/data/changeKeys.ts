export type Versions = { epoch: string; domains: Record<string, number> }

const keysByDomain: Record<string, string[]> = {
  contacts: ['contacts', 'contact', 'lead'],
  activity: ['contacts', 'contact', 'activities', 'dashboard'],
  leads: ['leads', 'lead'],
  mail: ['mail-drafts', 'mail-draft', 'mail-accounts', 'inbox', 'inbox-record', 'inbox-thread', 'imap-accounts', 'dashboard'],
  mail_expiration: ['mail-drafts', 'mail-draft', 'dashboard'],
  configuration: ['crm-states', 'contacts', 'contact'],
  audit: ['audit'],
  custom_fields: ['crm-fields', 'contact-fields', 'contacts', 'crm-views'],
  crm_views: ['crm-views'],
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
