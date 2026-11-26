ALTER TABLE mail_agent_clients ADD COLUMN collaboration_scopes TEXT NOT NULL DEFAULT '[]'
    CHECK (json_valid(collaboration_scopes) AND json_type(collaboration_scopes) = 'array');

CREATE TABLE agent_collaboration_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    require_registered_crm INTEGER NOT NULL DEFAULT 0 CHECK (require_registered_crm IN (0, 1)),
    require_registered_mail INTEGER NOT NULL DEFAULT 0 CHECK (require_registered_mail IN (0, 1))
);
INSERT INTO agent_collaboration_settings(id) VALUES (1);
CREATE TRIGGER notify_agent_collaboration_settings AFTER UPDATE ON agent_collaboration_settings BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='mail_agent_policy';
END;

ALTER TABLE agent_requests ADD COLUMN actor_id TEXT;
ALTER TABLE audit_logs ADD COLUMN task_id TEXT;

ALTER TABLE customer_documents ADD COLUMN created_by TEXT NOT NULL DEFAULT 'desktop-user';
ALTER TABLE customer_documents ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'
    CHECK (source IN ('manual', 'agent'));

ALTER TABLE activities ADD COLUMN record_revision INTEGER NOT NULL DEFAULT 1;

CREATE TABLE contact_revision_history (
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    revision INTEGER NOT NULL,
    actor_id TEXT NOT NULL,
    occurred_at INTEGER NOT NULL,
    before_json TEXT NOT NULL,
    after_json TEXT NOT NULL,
    PRIMARY KEY (contact_id, revision)
);

CREATE TRIGGER contact_revision_history_update AFTER UPDATE ON contacts
WHEN NEW.record_revision > OLD.record_revision AND (
    NEW.display_name IS NOT OLD.display_name
    OR NEW.company_name IS NOT OLD.company_name
    OR NEW.country_region IS NOT OLD.country_region
    OR NEW.stage IS NOT OLD.stage
    OR NEW.customer_level IS NOT OLD.customer_level
    OR NEW.primary_language IS NOT OLD.primary_language
    OR NEW.main_need IS NOT OLD.main_need
    OR NEW.sample_status IS NOT OLD.sample_status
    OR NEW.notes IS NOT OLD.notes
    OR NEW.archived_at IS NOT OLD.archived_at
)
BEGIN
    INSERT INTO contact_revision_history(contact_id,revision,actor_id,occurred_at,before_json,after_json)
    VALUES (NEW.id, NEW.record_revision, NEW.updated_by, unixepoch(),
        json_object('display_name', OLD.display_name,
            'company_name', OLD.company_name,
            'country_region', OLD.country_region,
            'stage', OLD.stage,
            'customer_level', OLD.customer_level,
            'primary_language', OLD.primary_language,
            'main_need', OLD.main_need,
            'sample_status', OLD.sample_status,
            'notes', OLD.notes,
            'archived_at', OLD.archived_at),
        json_object('display_name', NEW.display_name,
            'company_name', NEW.company_name,
            'country_region', NEW.country_region,
            'stage', NEW.stage,
            'customer_level', NEW.customer_level,
            'primary_language', NEW.primary_language,
            'main_need', NEW.main_need,
            'sample_status', NEW.sample_status,
            'notes', NEW.notes,
            'archived_at', NEW.archived_at));
END;
