CREATE TABLE agent_requests (
    operation TEXT NOT NULL, request_id TEXT NOT NULL, payload_sha256 TEXT NOT NULL,
    response_json TEXT, created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY(operation,request_id)
);

ALTER TABLE email_accounts ADD COLUMN daily_limit INTEGER NOT NULL DEFAULT 30 CHECK(daily_limit BETWEEN 1 AND 500);
ALTER TABLE email_accounts ADD COLUMN signature TEXT NOT NULL DEFAULT '';
CREATE INDEX email_outbox_attempt_idx ON email_outbox_jobs(last_attempt_at) WHERE last_attempt_at IS NOT NULL;
