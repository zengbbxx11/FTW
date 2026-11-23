CREATE TABLE imap_accounts (
    account_id TEXT PRIMARY KEY NOT NULL REFERENCES email_accounts(id),
    host TEXT NOT NULL, port INTEGER NOT NULL CHECK(port BETWEEN 1 AND 65535),
    username TEXT NOT NULL, secret_ref TEXT NOT NULL UNIQUE REFERENCES local_secrets(id),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE TABLE imap_sync_state (
    account_id TEXT NOT NULL REFERENCES email_accounts(id), folder TEXT NOT NULL,
    uid_validity INTEGER NOT NULL, last_uid INTEGER NOT NULL, last_sync INTEGER, outcome TEXT,
    PRIMARY KEY(account_id, folder)
);
CREATE TABLE inbox_messages (
    id TEXT PRIMARY KEY NOT NULL, account_id TEXT NOT NULL REFERENCES email_accounts(id),
    contact_id TEXT REFERENCES contacts(id), thread_id TEXT NOT NULL,
    message_id_header TEXT NOT NULL, raw_sha256 TEXT NOT NULL,
    sender TEXT NOT NULL, subject TEXT NOT NULL, body_text TEXT NOT NULL,
    attachment_names TEXT NOT NULL CHECK(json_valid(attachment_names)),
    received_at INTEGER NOT NULL, raw BLOB NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('message','delivery_report','large_message','unparsed')),
    UNIQUE(account_id, raw_sha256)
);
CREATE INDEX inbox_account_header ON inbox_messages(account_id, message_id_header);
CREATE INDEX inbox_contact_received ON inbox_messages(contact_id, received_at DESC, id);
CREATE INDEX inbox_thread_order ON inbox_messages(thread_id, received_at, id);
CREATE TABLE inbox_locations (
    account_id TEXT NOT NULL REFERENCES email_accounts(id), folder TEXT NOT NULL,
    uid_validity INTEGER NOT NULL, uid INTEGER NOT NULL,
    message_id TEXT NOT NULL REFERENCES inbox_messages(id),
    PRIMARY KEY (account_id, folder, uid_validity, uid)
);
