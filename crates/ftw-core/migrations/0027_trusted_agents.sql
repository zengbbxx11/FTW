ALTER TABLE mail_agent_clients ADD COLUMN paused INTEGER NOT NULL DEFAULT 0 CHECK (paused IN (0, 1));
ALTER TABLE mail_agent_clients ADD COLUMN last_seen_at INTEGER;

DROP TRIGGER notify_mail_agent_client_update;
CREATE TRIGGER notify_mail_agent_client_update
AFTER UPDATE OF revoked, automatic, paused ON mail_agent_clients BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_agent_policy';
END;
