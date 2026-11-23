CREATE TABLE crm_fields (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('text','number','date','single_select','multi_select','checkbox','url')),
    color TEXT NOT NULL,
    position INTEGER NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
    agent_access TEXT NOT NULL DEFAULT 'read' CHECK(agent_access IN ('hidden','read','write')),
    show_in_details INTEGER NOT NULL DEFAULT 1 CHECK(show_in_details IN (0,1)),
    default_json TEXT CHECK(default_json IS NULL OR json_valid(default_json)),
    UNIQUE(id,kind)
);
CREATE UNIQUE INDEX crm_field_active_name ON crm_fields(name COLLATE NOCASE) WHERE archived=0;

CREATE TABLE crm_field_options (
    field_id TEXT NOT NULL REFERENCES crm_fields(id),
    id TEXT NOT NULL,
    label TEXT NOT NULL,
    color TEXT NOT NULL,
    position INTEGER NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)),
    PRIMARY KEY(field_id,id)
);
CREATE UNIQUE INDEX crm_field_active_option ON crm_field_options(field_id,label COLLATE NOCASE) WHERE archived=0;

CREATE TABLE crm_field_values (
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    field_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    text_value TEXT,
    number_value REAL,
    bool_value INTEGER CHECK(bool_value IN (0,1)),
    option_id TEXT,
    PRIMARY KEY(contact_id,field_id),
    FOREIGN KEY(field_id,kind) REFERENCES crm_fields(id,kind),
    FOREIGN KEY(field_id,option_id) REFERENCES crm_field_options(field_id,id),
    CHECK((kind IN ('text','url','date')) = (text_value IS NOT NULL)),
    CHECK((kind='number') = (number_value IS NOT NULL)),
    CHECK((kind='checkbox') = (bool_value IS NOT NULL)),
    CHECK((kind='single_select') = (option_id IS NOT NULL))
);
CREATE INDEX crm_custom_text_index ON crm_field_values(field_id,text_value COLLATE NOCASE,contact_id)
    WHERE text_value IS NOT NULL;
CREATE INDEX crm_custom_number_index ON crm_field_values(field_id,number_value,contact_id)
    WHERE number_value IS NOT NULL;
CREATE INDEX crm_custom_bool_index ON crm_field_values(field_id,bool_value,contact_id)
    WHERE bool_value IS NOT NULL;
CREATE INDEX crm_custom_option_index ON crm_field_values(field_id,option_id,contact_id)
    WHERE option_id IS NOT NULL;

CREATE TABLE crm_field_multi_values (
    contact_id TEXT NOT NULL,
    field_id TEXT NOT NULL,
    option_id TEXT NOT NULL,
    PRIMARY KEY(contact_id,field_id,option_id),
    FOREIGN KEY(contact_id,field_id) REFERENCES crm_field_values(contact_id,field_id) ON DELETE CASCADE,
    FOREIGN KEY(field_id,option_id) REFERENCES crm_field_options(field_id,id)
);
CREATE INDEX crm_custom_multi_index ON crm_field_multi_values(field_id,option_id,contact_id);

INSERT INTO app_change_counters(domain) VALUES ('custom_fields');
CREATE TRIGGER notify_crm_fields_insert AFTER INSERT ON crm_fields
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='custom_fields';
END;
CREATE TRIGGER notify_crm_fields_update AFTER UPDATE ON crm_fields
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='custom_fields';
END;
