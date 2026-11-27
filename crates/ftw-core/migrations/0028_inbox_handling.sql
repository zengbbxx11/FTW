ALTER TABLE inbox_messages ADD COLUMN non_customer INTEGER NOT NULL DEFAULT 0 CHECK(non_customer IN (0,1));
ALTER TABLE inbox_messages ADD COLUMN handling_revision INTEGER NOT NULL DEFAULT 1;
CREATE INDEX inbox_handling ON inbox_messages(non_customer,received_at);
CREATE TRIGGER notify_crm_view_delete AFTER DELETE ON crm_views BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='crm_views';
END;
