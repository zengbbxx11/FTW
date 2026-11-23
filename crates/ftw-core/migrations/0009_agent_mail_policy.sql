CREATE TABLE mail_agent_policy (
    id INTEGER PRIMARY KEY CHECK (id=1),
    mode TEXT NOT NULL CHECK(mode IN ('manual','disabled','automatic')),
    revision INTEGER NOT NULL,
    accounts_json TEXT NOT NULL CHECK(json_valid(accounts_json)),
    purposes_json TEXT NOT NULL CHECK(json_valid(purposes_json)),
    daily_limit INTEGER NOT NULL CHECK(daily_limit BETWEEN 1 AND 30),
    task_limit INTEGER NOT NULL CHECK(task_limit BETWEEN 1 AND 20),
    interval_seconds INTEGER NOT NULL CHECK(interval_seconds BETWEEN 60 AND 3600),
    ledger_required INTEGER NOT NULL DEFAULT 0 CHECK(ledger_required IN (0,1))
);
INSERT INTO mail_agent_policy VALUES (1,'manual',1,'[]','[]',30,20,60,0);
CREATE TABLE mail_agent_clients (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    token_digest BLOB NOT NULL UNIQUE,
    revoked INTEGER NOT NULL DEFAULT 0 CHECK(revoked IN (0,1))
);
CREATE TABLE mail_agent_reviews (
    id TEXT PRIMARY KEY NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('waiting','saved','cancelled','failed')),
    outcome TEXT,
    changed_at INTEGER NOT NULL DEFAULT(unixepoch())
);

CREATE TEMP TABLE keep_email_messages AS SELECT * FROM email_messages;
DROP TABLE email_messages;
ALTER TABLE email_outbox_jobs RENAME TO previous_email_outbox_jobs;
CREATE TABLE email_outbox_jobs (
    id TEXT PRIMARY KEY NOT NULL, draft_id TEXT NOT NULL REFERENCES email_drafts(id),
    approval_id TEXT UNIQUE REFERENCES email_approvals(id),
    policy_revision INTEGER,
    queued_source TEXT NOT NULL DEFAULT 'manual' CHECK(queued_source IN ('manual','agent')),
    agent_id TEXT REFERENCES mail_agent_clients(id),
    task_id TEXT,
    request_id TEXT,
    content_sha256 TEXT,
    purpose TEXT NOT NULL DEFAULT 'legacy_unclassified',
    normalized_recipient TEXT,
    revision INTEGER NOT NULL, message_id_header TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK(state IN ('queued','sending','smtp_accepted','failed','delivery_unknown')),
    attempt_count INTEGER NOT NULL DEFAULT 0, outcome TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()), last_attempt_at INTEGER,
    CHECK ((approval_id IS NOT NULL AND policy_revision IS NULL) OR
           (approval_id IS NULL AND policy_revision IS NOT NULL AND agent_id IS NOT NULL AND
            task_id IS NOT NULL AND request_id IS NOT NULL AND content_sha256 IS NOT NULL AND normalized_recipient IS NOT NULL)),
    UNIQUE (draft_id, revision)
);

INSERT INTO email_outbox_jobs(id,draft_id,approval_id,revision,message_id_header,state,attempt_count,outcome,created_at,last_attempt_at)
SELECT id,draft_id,approval_id,revision,message_id_header,state,attempt_count,outcome,created_at,last_attempt_at FROM previous_email_outbox_jobs;
DROP TABLE previous_email_outbox_jobs;
CREATE INDEX email_outbox_queue ON email_outbox_jobs(state, created_at, id);
CREATE INDEX email_outbox_attempt_idx ON email_outbox_jobs(last_attempt_at) WHERE last_attempt_at IS NOT NULL;
CREATE UNIQUE INDEX email_agent_request ON email_outbox_jobs(agent_id,request_id) WHERE agent_id IS NOT NULL;
CREATE UNIQUE INDEX email_first_outreach ON email_outbox_jobs(normalized_recipient)
WHERE purpose='first_outreach' AND state IN ('queued','sending','smtp_accepted','delivery_unknown');
CREATE INDEX email_recipient_history ON email_drafts(lower(recipient),state);
CREATE INDEX email_agent_limits ON email_outbox_jobs(agent_id,task_id,created_at) WHERE agent_id IS NOT NULL;
CREATE TABLE email_messages (
    id TEXT PRIMARY KEY NOT NULL, contact_id TEXT NOT NULL REFERENCES contacts(id),
    account_id TEXT NOT NULL REFERENCES email_accounts(id),
    outbox_id TEXT NOT NULL UNIQUE REFERENCES email_outbox_jobs(id),
    message_id_header TEXT NOT NULL UNIQUE, from_address TEXT NOT NULL, to_address TEXT NOT NULL,
    subject TEXT NOT NULL, body_text TEXT NOT NULL, submitted_at INTEGER NOT NULL
);
INSERT INTO email_messages SELECT * FROM keep_email_messages;
CREATE INDEX email_sent_recipient ON email_messages(lower(to_address));
DROP TABLE keep_email_messages;

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
