ALTER TABLE inbox_messages ADD COLUMN message_role TEXT NOT NULL DEFAULT 'legacy'
    CHECK(message_role IN ('legacy','correspondence','automatic','delivery_report','unparsed'));
ALTER TABLE inbox_messages ADD COLUMN match_state TEXT NOT NULL DEFAULT 'legacy'
    CHECK(match_state IN ('legacy','pending','contact','thread','manual','delivery'));
ALTER TABLE inbox_messages ADD COLUMN body_complete INTEGER NOT NULL DEFAULT 1 CHECK(body_complete IN (0,1));
UPDATE inbox_messages SET body_complete=0 WHERE kind='large_message';
UPDATE inbox_messages SET message_role='delivery_report' WHERE kind='delivery_report';
UPDATE inbox_messages SET message_role='unparsed' WHERE kind='unparsed';

CREATE TABLE mail_delivery_events (
    inbox_id TEXT PRIMARY KEY NOT NULL REFERENCES inbox_messages(id),
    outbox_id TEXT NOT NULL REFERENCES email_outbox_jobs(id),
    action TEXT NOT NULL CHECK(action IN ('failed','delayed')),
    status TEXT NOT NULL
);
CREATE INDEX mail_delivery_outbox ON mail_delivery_events(outbox_id,action);
CREATE INDEX inbox_linked_correspondence ON inbox_messages(contact_id,received_at DESC)
    WHERE message_role='correspondence' AND match_state IN ('thread','manual');
CREATE INDEX email_contact_submitted ON email_messages(contact_id,submitted_at DESC,id);

CREATE VIEW mail_contact_outreach AS
WITH facts AS (
    SELECT c.id contact_id,
        EXISTS(SELECT 1 FROM contact_methods m JOIN suppression_list s
            ON s.normalized=m.normalized OR s.normalized=substr(m.normalized,instr(m.normalized,'@')+1)
            WHERE m.contact_id=c.id AND m.kind='email') suppressed,
        (SELECT id FROM email_drafts WHERE contact_id=c.id AND state<>'draft'
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

INSERT OR IGNORE INTO crm_views(id,name,position,config_json)
VALUES ('mail_no_reply','已发未见回复',100,'{"query":{"filter":{"type":"rule","field":"mail_status","operator":"eq","value":"sent_no_reply_seen"}}}'),
       ('mail_replied','已回复',101,'{"query":{"filter":{"type":"rule","field":"mail_status","operator":"eq","value":"replied"}}}'),
       ('mail_bounced','已退信',102,'{"query":{"filter":{"type":"rule","field":"mail_status","operator":"eq","value":"bounced"}}}');

DROP VIEW mail_drafts_review;
CREATE VIEW mail_drafts_review AS SELECT d.rowid,d.*,
    (d.state='approved' AND NOT EXISTS(SELECT 1 FROM email_approvals a WHERE a.draft_id=d.id
        AND a.revision=d.revision AND a.expires_at>unixepoch())) approval_expired,
    COALESCE(j.purpose,'legacy_unclassified') purpose,j.id job_id,j.task_id,a.name agent_name
FROM email_drafts d LEFT JOIN email_outbox_jobs j ON j.draft_id=d.id AND j.revision=d.revision
LEFT JOIN mail_agent_clients a ON a.id=j.agent_id;
