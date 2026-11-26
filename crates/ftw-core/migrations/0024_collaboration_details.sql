CREATE TABLE contact_custom_history (
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    revision INTEGER NOT NULL,
    field_id TEXT NOT NULL REFERENCES crm_fields(id),
    field_name TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('manual','agent')),
    task_id TEXT,
    before_json TEXT NOT NULL CHECK (json_valid(before_json)),
    after_json TEXT NOT NULL CHECK (json_valid(after_json)),
    occurred_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (contact_id, revision, field_id)
);

CREATE TABLE customer_document_uploads (
    actor_id TEXT NOT NULL REFERENCES mail_agent_clients(id),
    upload_id TEXT NOT NULL,
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    filename TEXT NOT NULL,
    total_size INTEGER NOT NULL CHECK(total_size BETWEEN 1 AND 10485760),
    next_offset INTEGER NOT NULL DEFAULT 0 CHECK(next_offset BETWEEN 0 AND total_size),
    document_id TEXT REFERENCES customer_documents(id),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (actor_id, upload_id)
);

CREATE TABLE customer_document_upload_parts (
    actor_id TEXT NOT NULL,
    upload_id TEXT NOT NULL,
    byte_offset INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    bytes BLOB NOT NULL,
    PRIMARY KEY (actor_id, upload_id, byte_offset),
    FOREIGN KEY (actor_id, upload_id) REFERENCES customer_document_uploads(actor_id, upload_id) ON DELETE CASCADE
);

CREATE INDEX customer_document_upload_expiry ON customer_document_uploads(updated_at);

CREATE TABLE inbox_link_suggestions (
    message_id TEXT NOT NULL REFERENCES inbox_messages(id) ON DELETE CASCADE,
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    actor_id TEXT NOT NULL REFERENCES mail_agent_clients(id),
    occurred_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (message_id,contact_id,actor_id)
);

CREATE TRIGGER notify_inbox_link_suggestions_insert AFTER INSERT ON inbox_link_suggestions BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;

CREATE TRIGGER notify_inbox_link_suggestions_delete AFTER DELETE ON inbox_link_suggestions BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail';
END;
