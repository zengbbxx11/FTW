ALTER TABLE contacts ADD COLUMN record_revision INTEGER NOT NULL DEFAULT 1;
ALTER TABLE activities ADD COLUMN counts_as_contact INTEGER CHECK(counts_as_contact IN (0,1));

CREATE TABLE app_change_epoch (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    epoch TEXT NOT NULL
);
INSERT INTO app_change_epoch VALUES (1, lower(hex(randomblob(16))));

CREATE TABLE app_change_counters (
    domain TEXT PRIMARY KEY,
    revision INTEGER NOT NULL DEFAULT 0
);
INSERT INTO app_change_counters(domain) VALUES ('contacts'),('activity'),('leads'),('mail'),('configuration'),('audit');

CREATE TABLE crm_state_options (
    field TEXT NOT NULL,
    code TEXT NOT NULL,
    label TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY(field,code)
);
CREATE TABLE crm_state_aliases (
    field TEXT NOT NULL,
    alias TEXT NOT NULL COLLATE NOCASE,
    code TEXT NOT NULL,
    PRIMARY KEY(field,alias),
    FOREIGN KEY(field,code) REFERENCES crm_state_options(field,code)
);

CREATE TABLE email_approval_attempts (
    id TEXT PRIMARY KEY,
    draft_id TEXT NOT NULL REFERENCES email_drafts(id),
    revision INTEGER NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('waiting','approved','cancelled','failed','interrupted')),
    outcome TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    finished_at INTEGER
);
CREATE INDEX approval_attempt_draft ON email_approval_attempts(draft_id,revision,created_at DESC);

CREATE VIEW crm_contact_dates AS
WITH communication AS (
    SELECT c.id, c.first_contact_at, c.last_contact_at,
      (SELECT MIN(a.occurred_at) FROM activities a WHERE a.contact_id=c.id
        AND (COALESCE(a.counts_as_contact,a.event_type IN ('call','meeting','chat','sample','email_sent')) OR
          (a.event_type='email_received' AND EXISTS(SELECT 1 FROM inbox_messages m
            WHERE m.id=a.related_message_id AND m.kind IN ('message','large_message'))))) observed_first,
      (SELECT MAX(a.occurred_at) FROM activities a WHERE a.contact_id=c.id
        AND (COALESCE(a.counts_as_contact,a.event_type IN ('call','meeting','chat','sample','email_sent')) OR
          (a.event_type='email_received' AND EXISTS(SELECT 1 FROM inbox_messages m
            WHERE m.id=a.related_message_id AND m.kind IN ('message','large_message'))))) observed_last
    FROM contacts c
)
SELECT id, observed_first, observed_last,
  (SELECT MIN(value) FROM (SELECT first_contact_at value UNION ALL SELECT last_contact_at UNION ALL SELECT observed_first)) effective_first,
  (SELECT MAX(value) FROM (SELECT first_contact_at value UNION ALL SELECT last_contact_at UNION ALL SELECT observed_last)) effective_last
FROM communication;

INSERT INTO crm_state_options(field,code,label,position) VALUES
    ('customer_level','A','重点客户',0),
    ('customer_level','B','B 级',1),
    ('customer_level','C','C 级',2);
INSERT INTO crm_state_aliases(field,alias,code) VALUES
    ('customer_level','a','A'),
    ('customer_level','重点客户','A'),
    ('customer_level','a级','A'),
    ('customer_level','重点','A'),
    ('customer_level','priority','A'),
    ('customer_level','b','B'),
    ('customer_level','b 级','B'),
    ('customer_level','b级','B'),
    ('customer_level','c','C'),
    ('customer_level','c 级','C'),
    ('customer_level','c级','C');

INSERT INTO crm_state_options(field,code,label,position) VALUES
    ('customer_type','distributor','分销商',0),
    ('customer_type','wholesaler','批发商',1),
    ('customer_type','retailer','零售商',2),
    ('customer_type','ecommerce','电商卖家',3),
    ('customer_type','brand','品牌采购商',4),
    ('customer_type','oem_odm','OEM / ODM',5);
INSERT INTO crm_state_aliases(field,alias,code) VALUES
    ('customer_type','distributor','distributor'),
    ('customer_type','分销商','distributor'),
    ('customer_type','经销商','distributor'),
    ('customer_type','wholesaler','wholesaler'),
    ('customer_type','批发商','wholesaler'),
    ('customer_type','retailer','retailer'),
    ('customer_type','零售商','retailer'),
    ('customer_type','线下零售商','retailer'),
    ('customer_type','ecommerce','ecommerce'),
    ('customer_type','电商卖家','ecommerce'),
    ('customer_type','brand','brand'),
    ('customer_type','品牌采购商','brand'),
    ('customer_type','oem_odm','oem_odm'),
    ('customer_type','oem / odm','oem_odm'),
    ('customer_type','oem','oem_odm'),
    ('customer_type','odm','oem_odm'),
    ('customer_type','oem/odm','oem_odm');

INSERT INTO crm_state_options(field,code,label,position) VALUES
    ('customer_source','website','官网',0),
    ('customer_source','trade_show','展会',1),
    ('customer_source','canton_fair','广交会',2),
    ('customer_source','alibaba','阿里巴巴',3),
    ('customer_source','referral','推荐',4),
    ('customer_source','inbound','询盘',5),
    ('customer_source','agent_research','Agent 调查',6);
INSERT INTO crm_state_aliases(field,alias,code) VALUES
    ('customer_source','website','website'),
    ('customer_source','官网','website'),
    ('customer_source','公司官网','website'),
    ('customer_source','公司官网询盘','website'),
    ('customer_source','trade_show','trade_show'),
    ('customer_source','展会','trade_show'),
    ('customer_source','canton_fair','canton_fair'),
    ('customer_source','广交会','canton_fair'),
    ('customer_source','alibaba','alibaba'),
    ('customer_source','阿里巴巴','alibaba'),
    ('customer_source','referral','referral'),
    ('customer_source','推荐','referral'),
    ('customer_source','inbound','inbound'),
    ('customer_source','询盘','inbound'),
    ('customer_source','agent_research','agent_research'),
    ('customer_source','agent 调查','agent_research');

INSERT INTO crm_state_options(field,code,label,position) VALUES
    ('sample_status','none','未涉及',0),
    ('sample_status','not_sent','未寄样',1),
    ('sample_status','planned','待安排寄样',2),
    ('sample_status','in_progress','样品推进中',3),
    ('sample_status','in_transit','样品在途',4),
    ('sample_status','evaluating','样品评估中',5),
    ('sample_status','passed','样品通过',6),
    ('sample_status','complete','样品已完成',7),
    ('sample_status','failed','样品未通过',8);
INSERT INTO crm_state_aliases(field,alias,code) VALUES
    ('sample_status','none','none'),
    ('sample_status','未涉及','none'),
    ('sample_status','无样品','none'),
    ('sample_status','不需要样品','none'),
    ('sample_status','not_sent','not_sent'),
    ('sample_status','未寄样','not_sent'),
    ('sample_status','planned','planned'),
    ('sample_status','待安排寄样','planned'),
    ('sample_status','准备寄样','planned'),
    ('sample_status','待寄样','planned'),
    ('sample_status','in_progress','in_progress'),
    ('sample_status','样品推进中','in_progress'),
    ('sample_status','in_transit','in_transit'),
    ('sample_status','样品在途','in_transit'),
    ('sample_status','样品运输中','in_transit'),
    ('sample_status','已寄样','in_transit'),
    ('sample_status','evaluating','evaluating'),
    ('sample_status','样品评估中','evaluating'),
    ('sample_status','样品测试中','evaluating'),
    ('sample_status','测试中','evaluating'),
    ('sample_status','passed','passed'),
    ('sample_status','样品通过','passed'),
    ('sample_status','complete','complete'),
    ('sample_status','样品已完成','complete'),
    ('sample_status','failed','failed'),
    ('sample_status','样品未通过','failed');

INSERT INTO crm_state_options(field,code,label,position) VALUES
    ('bulk_order_status','none','未开始',0),
    ('bulk_order_status','inquiry','初步询盘',1),
    ('bulk_order_status','discussing','批量采购洽谈中',2),
    ('bulk_order_status','confirmed','批量采购已确认',3),
    ('bulk_order_status','completed','批量采购已完成',4);
INSERT INTO crm_state_aliases(field,alias,code) VALUES
    ('bulk_order_status','none','none'),
    ('bulk_order_status','未开始','none'),
    ('bulk_order_status','inquiry','inquiry'),
    ('bulk_order_status','初步询盘','inquiry'),
    ('bulk_order_status','discussing','discussing'),
    ('bulk_order_status','批量采购洽谈中','discussing'),
    ('bulk_order_status','confirmed','confirmed'),
    ('bulk_order_status','批量采购已确认','confirmed'),
    ('bulk_order_status','completed','completed'),
    ('bulk_order_status','批量采购已完成','completed');

CREATE TRIGGER notify_contacts_insert AFTER INSERT ON contacts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_contacts_update AFTER UPDATE ON contacts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_contacts_delete AFTER DELETE ON contacts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_contact_methods_insert AFTER INSERT ON contact_methods
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_contact_methods_update AFTER UPDATE ON contact_methods
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_contact_methods_delete AFTER DELETE ON contact_methods
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_customer_interests_insert AFTER INSERT ON customer_interests
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_customer_interests_update AFTER UPDATE ON customer_interests
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_customer_interests_delete AFTER DELETE ON customer_interests
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;

CREATE TRIGGER notify_activities_insert AFTER INSERT ON activities
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='activity';
END;

CREATE TRIGGER notify_activities_update AFTER UPDATE ON activities
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='activity';
END;

CREATE TRIGGER notify_activities_delete AFTER DELETE ON activities
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='activity';
END;

CREATE TRIGGER notify_lead_candidates_insert AFTER INSERT ON lead_candidates
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='leads';
END;

CREATE TRIGGER notify_lead_candidates_update AFTER UPDATE ON lead_candidates
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='leads';
END;

CREATE TRIGGER notify_lead_candidates_delete AFTER DELETE ON lead_candidates
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='leads';
END;

CREATE TRIGGER notify_email_accounts_insert AFTER INSERT ON email_accounts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_accounts_update AFTER UPDATE ON email_accounts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_accounts_delete AFTER DELETE ON email_accounts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_drafts_insert AFTER INSERT ON email_drafts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_drafts_update AFTER UPDATE ON email_drafts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_drafts_delete AFTER DELETE ON email_drafts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_approvals_insert AFTER INSERT ON email_approvals
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_approvals_update AFTER UPDATE ON email_approvals
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_approvals_delete AFTER DELETE ON email_approvals
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_outbox_jobs_insert AFTER INSERT ON email_outbox_jobs
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_outbox_jobs_update AFTER UPDATE ON email_outbox_jobs
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_outbox_jobs_delete AFTER DELETE ON email_outbox_jobs
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_messages_insert AFTER INSERT ON email_messages
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_messages_update AFTER UPDATE ON email_messages
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_messages_delete AFTER DELETE ON email_messages
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_imap_accounts_insert AFTER INSERT ON imap_accounts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_imap_accounts_update AFTER UPDATE ON imap_accounts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_imap_accounts_delete AFTER DELETE ON imap_accounts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_imap_sync_state_insert AFTER INSERT ON imap_sync_state
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_imap_sync_state_update AFTER UPDATE ON imap_sync_state
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_imap_sync_state_delete AFTER DELETE ON imap_sync_state
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_inbox_messages_insert AFTER INSERT ON inbox_messages
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_inbox_messages_update AFTER UPDATE ON inbox_messages
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_inbox_messages_delete AFTER DELETE ON inbox_messages
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_suppression_list_insert AFTER INSERT ON suppression_list
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_suppression_list_update AFTER UPDATE ON suppression_list
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_suppression_list_delete AFTER DELETE ON suppression_list
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_approval_attempts_insert AFTER INSERT ON email_approval_attempts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_approval_attempts_update AFTER UPDATE ON email_approval_attempts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_email_approval_attempts_delete AFTER DELETE ON email_approval_attempts
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_crm_state_options_insert AFTER INSERT ON crm_state_options
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='configuration';
END;

CREATE TRIGGER notify_crm_state_options_update AFTER UPDATE ON crm_state_options
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='configuration';
END;

CREATE TRIGGER notify_crm_state_options_delete AFTER DELETE ON crm_state_options
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='configuration';
END;

CREATE TRIGGER notify_crm_state_aliases_insert AFTER INSERT ON crm_state_aliases
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='configuration';
END;

CREATE TRIGGER notify_crm_state_aliases_update AFTER UPDATE ON crm_state_aliases
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='configuration';
END;

CREATE TRIGGER notify_crm_state_aliases_delete AFTER DELETE ON crm_state_aliases
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='configuration';
END;

CREATE TRIGGER notify_audit_logs_insert AFTER INSERT ON audit_logs
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='audit';
END;

CREATE TRIGGER notify_audit_logs_update AFTER UPDATE ON audit_logs
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='audit';
END;

CREATE TRIGGER notify_audit_logs_delete AFTER DELETE ON audit_logs
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='audit';
END;

CREATE VIEW mail_drafts_review AS SELECT d.rowid,d.*,
    (d.state='approved' AND NOT EXISTS(SELECT 1 FROM email_approvals a WHERE a.draft_id=d.id
        AND a.revision=d.revision AND a.expires_at>unixepoch())) approval_expired
FROM email_drafts d;

CREATE INDEX email_drafts_approved ON email_drafts(id,revision) WHERE state='approved';
