CREATE TABLE crm_company_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name))>0),
    website TEXT NOT NULL DEFAULT '',
    notes TEXT NOT NULL DEFAULT '',
    aliases_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(aliases_json) AND json_type(aliases_json)='array'),
    revision INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX crm_company_profiles_name ON crm_company_profiles(name COLLATE NOCASE,id);

CREATE TABLE crm_company_memberships (
    contact_id TEXT PRIMARY KEY NOT NULL REFERENCES contacts(id),
    company_id TEXT NOT NULL REFERENCES crm_company_profiles(id),
    branch TEXT NOT NULL DEFAULT ''
);
CREATE INDEX crm_company_memberships_company ON crm_company_memberships(company_id,contact_id);

CREATE TRIGGER notify_company_profiles_insert AFTER INSERT ON crm_company_profiles
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;
CREATE TRIGGER notify_company_profiles_update AFTER UPDATE ON crm_company_profiles
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='contacts';
END;
