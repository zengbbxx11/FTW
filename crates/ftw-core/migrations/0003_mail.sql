CREATE TABLE local_secrets (id TEXT PRIMARY KEY NOT NULL, protected BLOB NOT NULL);
CREATE TABLE email_accounts (
    id TEXT PRIMARY KEY NOT NULL, display_name TEXT NOT NULL, email_address TEXT NOT NULL,
    smtp_host TEXT NOT NULL, smtp_port INTEGER NOT NULL CHECK(smtp_port BETWEEN 1 AND 65535),
    smtp_security TEXT NOT NULL CHECK(smtp_security IN ('tls','starttls')),
    smtp_username TEXT NOT NULL, secret_ref TEXT NOT NULL UNIQUE REFERENCES local_secrets(id),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()), updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE email_drafts (
    id TEXT PRIMARY KEY NOT NULL, contact_id TEXT NOT NULL REFERENCES contacts(id),
    account_id TEXT NOT NULL REFERENCES email_accounts(id),
    recipient TEXT NOT NULL, subject TEXT NOT NULL, body_text TEXT NOT NULL,
    attachments_json TEXT NOT NULL CHECK(json_valid(attachments_json)),
    revision INTEGER NOT NULL DEFAULT 1,
    state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN
      ('draft','pending_approval','approved','queued','sending','smtp_accepted','failed','delivery_unknown')),
    source TEXT NOT NULL CHECK(source IN ('manual','agent')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()), updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX email_drafts_contact ON email_drafts(contact_id, updated_at DESC, id);
CREATE INDEX email_drafts_recent ON email_drafts(updated_at DESC, id);
CREATE TABLE email_approvals (
    id TEXT PRIMARY KEY NOT NULL, draft_id TEXT NOT NULL REFERENCES email_drafts(id),
    revision INTEGER NOT NULL, content_sha256 TEXT NOT NULL, approved_by_user TEXT NOT NULL,
    approved_at INTEGER NOT NULL DEFAULT (unixepoch()), expires_at INTEGER NOT NULL,
    UNIQUE(draft_id, revision)
);
CREATE TABLE email_outbox_jobs (
    id TEXT PRIMARY KEY NOT NULL, draft_id TEXT NOT NULL REFERENCES email_drafts(id),
    approval_id TEXT NOT NULL UNIQUE REFERENCES email_approvals(id),
    revision INTEGER NOT NULL, message_id_header TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK(state IN ('queued','sending','smtp_accepted','failed','delivery_unknown')),
    attempt_count INTEGER NOT NULL DEFAULT 0, outcome TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()), last_attempt_at INTEGER,
    UNIQUE (draft_id, revision)
);
CREATE INDEX email_outbox_queue ON email_outbox_jobs(state, created_at, id);
CREATE TABLE email_messages (
    id TEXT PRIMARY KEY NOT NULL, contact_id TEXT NOT NULL REFERENCES contacts(id),
    account_id TEXT NOT NULL REFERENCES email_accounts(id),
    outbox_id TEXT NOT NULL UNIQUE REFERENCES email_outbox_jobs(id),
    message_id_header TEXT NOT NULL UNIQUE, from_address TEXT NOT NULL, to_address TEXT NOT NULL,
    subject TEXT NOT NULL, body_text TEXT NOT NULL, submitted_at INTEGER NOT NULL
);
CREATE TABLE suppression_list (
    normalized TEXT PRIMARY KEY NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

ALTER TABLE activities ADD COLUMN channel TEXT NOT NULL DEFAULT '';
ALTER TABLE activities ADD COLUMN followup_category TEXT NOT NULL DEFAULT '';
ALTER TABLE activities ADD COLUMN customer_feedback TEXT NOT NULL DEFAULT '';
