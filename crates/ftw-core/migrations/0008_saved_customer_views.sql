CREATE TABLE crm_views (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    position INTEGER NOT NULL,
    hidden INTEGER NOT NULL DEFAULT 0 CHECK(hidden IN (0,1)),
    is_default INTEGER NOT NULL DEFAULT 0 CHECK(is_default IN (0,1)),
    archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    config_json TEXT NOT NULL CHECK(json_valid(config_json)),
    CHECK(NOT is_default OR (NOT hidden AND NOT archived))
);
CREATE UNIQUE INDEX crm_one_default_view ON crm_views(is_default) WHERE is_default=1;
CREATE UNIQUE INDEX crm_unique_active_view ON crm_views(name COLLATE NOCASE) WHERE NOT archived;
INSERT INTO crm_views(id,name,position,is_default,config_json)
VALUES ('all','全部客户',1,1,'{}'),
       ('priority','重点客户',2,0,'{"query":{"quick_view":"priority"}}'),
       ('sample','样品客户',3,0,'{"query":{"quick_view":"sample"}}');

INSERT INTO app_change_counters(domain) VALUES ('crm_views');
CREATE TRIGGER notify_crm_view_insert AFTER INSERT ON crm_views
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='crm_views';
END;
CREATE TRIGGER notify_crm_view_update AFTER UPDATE ON crm_views
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='crm_views';
END;
