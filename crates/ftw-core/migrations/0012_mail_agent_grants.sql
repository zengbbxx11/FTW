ALTER TABLE mail_agent_clients ADD COLUMN automatic INTEGER NOT NULL DEFAULT 0 CHECK(automatic IN (0,1));
UPDATE mail_agent_clients SET automatic=1
WHERE NOT revoked AND (SELECT mode FROM mail_agent_policy WHERE id=1)='automatic';

CREATE TRIGGER notify_mail_agent_client_insert AFTER INSERT ON mail_agent_clients BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_agent_policy';
END;
CREATE TRIGGER notify_mail_agent_client_update AFTER UPDATE ON mail_agent_clients BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_agent_policy';
END;
CREATE TRIGGER notify_mail_agent_policy_update AFTER UPDATE ON mail_agent_policy BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_agent_policy';
END;
