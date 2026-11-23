ALTER TABLE contacts ADD COLUMN merged_into TEXT REFERENCES contacts(id);
CREATE INDEX contacts_merged_into ON contacts(merged_into) WHERE merged_into IS NOT NULL;

CREATE TABLE crm_agent_changes (
    id TEXT PRIMARY KEY NOT NULL,
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    request_id TEXT NOT NULL,
    field TEXT NOT NULL CHECK(field IN ('stage','customer_level','primary_language','main_need','sample_status','notes')),
    previous_value TEXT NOT NULL,
    proposed_value TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('applied','pending','kept','accepted')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX crm_agent_changes_state ON crm_agent_changes(state,created_at DESC,id);

DROP VIEW mail_contact_outreach;
CREATE VIEW mail_contact_outreach AS
WITH facts AS (
    SELECT c.id contact_id,
        EXISTS(SELECT 1 FROM contact_methods m JOIN suppression_list s
            ON s.normalized=m.normalized OR s.normalized=substr(m.normalized,instr(m.normalized,'@')+1)
            WHERE m.contact_id=c.id AND m.kind='email') suppressed,
        (SELECT id FROM email_drafts WHERE (contact_id=c.id OR contact_id IN (SELECT id FROM contacts WHERE merged_into=c.id)) AND state<>'draft'
            ORDER BY updated_at DESC,rowid DESC LIMIT 1) latest_draft_id,
        (SELECT e.id FROM email_messages e
            WHERE e.contact_id=c.id ORDER BY submitted_at DESC,id DESC LIMIT 1) latest_message_id,
        (SELECT MAX(i.received_at) FROM inbox_messages i WHERE i.contact_id=c.id
            AND i.message_role='correspondence' AND i.match_state IN ('thread','manual')) last_reply_at,
        (SELECT COUNT(*) FROM email_messages e JOIN email_outbox_jobs o ON o.id=e.outbox_id
            WHERE e.contact_id=c.id AND o.purpose IN ('first_outreach','outreach_followup','legacy_unclassified')) outreach_count
    FROM contacts c
)
SELECT f.contact_id,
    CASE
        WHEN f.suppressed THEN 'do_not_contact'
        WHEN d.state='delivery_unknown' THEN 'delivery_unknown'
        WHEN d.state='failed' THEN 'failed'
        WHEN d.state IN ('pending_approval','approved','queued','sending') THEN 'pending'
        WHEN e.id IS NOT NULL AND EXISTS(SELECT 1 FROM mail_delivery_events b WHERE b.outbox_id=e.outbox_id AND b.action='failed') THEN 'bounced'
        WHEN f.last_reply_at IS NOT NULL THEN 'replied'
        WHEN e.id IS NOT NULL AND s.outcome='ok' AND s.last_sync>e.submitted_at THEN 'sent_no_reply_seen'
        WHEN e.id IS NOT NULL THEN 'sent_reply_pending_sync'
        ELSE 'not_started'
    END mail_status,
    e.submitted_at last_sent_at,f.last_reply_at,f.outreach_count,e.subject recent_subject,
    CASE WHEN e.id IS NOT NULL THEN 'sent:' || e.id END thread_id,s.outcome sync_outcome,s.last_sync sync_at
FROM facts f LEFT JOIN email_drafts d ON d.id=f.latest_draft_id
LEFT JOIN email_messages e ON e.id=f.latest_message_id
LEFT JOIN imap_sync_state s ON s.account_id=e.account_id AND s.folder='INBOX';
