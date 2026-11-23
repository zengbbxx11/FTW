CREATE TABLE contacts (
    search_id INTEGER PRIMARY KEY,
    id TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL CHECK(length(trim(display_name)) > 0),
    company_name TEXT NOT NULL, country_region TEXT NOT NULL,
    customer_type TEXT NOT NULL, customer_source TEXT NOT NULL, customer_level TEXT NOT NULL,
    stage TEXT NOT NULL,
    first_contact_channel TEXT NOT NULL, primary_language TEXT NOT NULL, main_need TEXT NOT NULL,
    sample_status TEXT NOT NULL, bulk_order_status TEXT NOT NULL,
    first_contact_at INTEGER, last_contact_at INTEGER,
    website_url TEXT NOT NULL, website_domain TEXT NOT NULL,
    alibaba_buyer_id TEXT NOT NULL, notes TEXT NOT NULL,
    archived_at INTEGER, source TEXT NOT NULL CHECK(source IN ('manual', 'agent')),
    created_by TEXT NOT NULL, updated_by TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK(first_contact_at IS NULL OR last_contact_at IS NULL OR first_contact_at <= last_contact_at)
);
CREATE INDEX contacts_inbox ON contacts(archived_at, updated_at DESC, id);
CREATE INDEX contacts_stage ON contacts(stage, archived_at);
CREATE INDEX contacts_country ON contacts(country_region, archived_at);
CREATE INDEX contacts_level ON contacts(customer_level, archived_at);
CREATE INDEX contacts_type ON contacts(customer_type, archived_at);
CREATE INDEX contacts_source ON contacts(customer_source, archived_at);
CREATE INDEX contacts_domain ON contacts(website_domain) WHERE website_domain <> '';
CREATE INDEX contacts_company_country ON contacts(company_name COLLATE NOCASE, country_region COLLATE NOCASE);
CREATE VIRTUAL TABLE contact_search USING fts5(searchable, tokenize='trigram');

CREATE TABLE contact_methods (
    id TEXT PRIMARY KEY NOT NULL, contact_id TEXT NOT NULL REFERENCES contacts(id),
    kind TEXT NOT NULL CHECK(kind IN ('email','phone','whatsapp','wechat','other')),
    value TEXT NOT NULL, normalized TEXT NOT NULL, label TEXT NOT NULL,
    is_primary INTEGER NOT NULL CHECK(is_primary IN (0,1)),
    is_verified INTEGER NOT NULL CHECK(is_verified IN (0,1)),
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(contact_id, kind, normalized)
);
CREATE INDEX contact_methods_email ON contact_methods(normalized, contact_id) WHERE kind = 'email';
CREATE INDEX contact_methods_contact ON contact_methods(contact_id, position);

CREATE TABLE customer_interests (
    id TEXT PRIMARY KEY NOT NULL, contact_id TEXT NOT NULL REFERENCES contacts(id),
    product_text TEXT NOT NULL, intent_quantity INTEGER CHECK(intent_quantity >= 0),
    quantity_is_estimate INTEGER NOT NULL CHECK(quantity_is_estimate IN (0,1)),
    requirement TEXT NOT NULL, position INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX contacts_interests ON customer_interests(contact_id, position);

CREATE TABLE lead_candidates (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL CHECK(length(trim(display_name)) > 0),
    company_name TEXT NOT NULL, country_region TEXT NOT NULL,
    email TEXT NOT NULL, email_normalized TEXT NOT NULL,
    website_url TEXT NOT NULL, website_domain TEXT NOT NULL,
    business_summary TEXT NOT NULL, source_urls_json TEXT NOT NULL, research_notes TEXT NOT NULL,
    confidence INTEGER CHECK(confidence BETWEEN 0 AND 100),
    review_status TEXT NOT NULL DEFAULT 'pending' CHECK(review_status IN ('pending','rejected','promoted')),
    source TEXT NOT NULL CHECK(source IN ('manual', 'agent')),
    created_by_agent_run_id TEXT,
    promoted_contact_id TEXT UNIQUE REFERENCES contacts(id),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK((review_status = 'promoted') = (promoted_contact_id IS NOT NULL))
);
CREATE INDEX leads_review ON lead_candidates(review_status, updated_at DESC, id);
CREATE INDEX leads_email ON lead_candidates(email_normalized) WHERE email_normalized <> '';
CREATE INDEX leads_domain ON lead_candidates(website_domain) WHERE website_domain <> '';
CREATE INDEX leads_company_country ON lead_candidates(company_name COLLATE NOCASE, country_region COLLATE NOCASE);

CREATE TABLE activities (
    id TEXT PRIMARY KEY NOT NULL, contact_id TEXT NOT NULL REFERENCES contacts(id),
    event_type TEXT NOT NULL CHECK(event_type IN ('note','call','meeting','chat','email_sent','email_received','sample','other')),
    occurred_at INTEGER NOT NULL, summary TEXT NOT NULL, details TEXT NOT NULL,
    related_message_id TEXT,
    source TEXT NOT NULL CHECK(source IN ('manual', 'agent', 'system')),
    actor_id TEXT NOT NULL, agent_run_id TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX activities_contact_time ON activities(contact_id, occurred_at DESC, id);
CREATE INDEX activities_time ON activities(occurred_at DESC, id);

CREATE TABLE audit_logs (
    id TEXT PRIMARY KEY NOT NULL, actor_id TEXT NOT NULL, source TEXT NOT NULL,
    entity TEXT NOT NULL, entity_id TEXT NOT NULL, operation TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX audit_entity ON audit_logs(entity, entity_id, created_at);
