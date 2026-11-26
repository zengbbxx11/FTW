CREATE TABLE mail_templates (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    purpose TEXT NOT NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    archived_at INTEGER,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX mail_templates_purpose ON mail_templates(purpose, archived_at, name COLLATE NOCASE, id);

CREATE TABLE customer_documents (
    id TEXT PRIMARY KEY NOT NULL,
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size BETWEEN 1 AND 10485760),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(contact_id, filename, sha256)
);

INSERT INTO app_change_counters(domain) VALUES ('mail_templates'), ('customer_documents');
CREATE TRIGGER notify_mail_templates_insert AFTER INSERT ON mail_templates BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_templates';
END;
CREATE TRIGGER notify_mail_templates_update AFTER UPDATE ON mail_templates BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_templates';
END;
CREATE TRIGGER notify_customer_documents_insert AFTER INSERT ON customer_documents BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='customer_documents';
END;
