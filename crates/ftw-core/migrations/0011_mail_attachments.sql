CREATE TABLE mail_assets (
    id TEXT PRIMARY KEY NOT NULL,
    filename TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size BETWEEN 1 AND 10485760),
    content_type TEXT NOT NULL,
    agent_allowed INTEGER NOT NULL DEFAULT 0 CHECK(agent_allowed IN (0,1)),
    UNIQUE(sha256,filename)
);
ALTER TABLE email_drafts ADD COLUMN asset_ids_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(asset_ids_json));
ALTER TABLE inbox_messages ADD COLUMN full_sha256 TEXT;
ALTER TABLE inbox_messages ADD COLUMN full_size INTEGER;
ALTER TABLE inbox_messages ADD COLUMN download_outcome TEXT;

INSERT INTO app_change_counters(domain) VALUES ('mail_assets'),('mail_agent_policy');
CREATE TRIGGER notify_mail_asset_insert AFTER INSERT ON mail_assets BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_assets';
END;
CREATE TRIGGER notify_mail_asset_update AFTER UPDATE ON mail_assets BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_assets';
END;
CREATE TRIGGER notify_mail_policy_review_insert AFTER INSERT ON mail_agent_reviews BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_agent_policy';
END;
CREATE TRIGGER notify_mail_policy_review_update AFTER UPDATE ON mail_agent_reviews BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_agent_policy';
END;
