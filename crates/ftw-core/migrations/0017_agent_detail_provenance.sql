ALTER TABLE crm_agent_changes RENAME TO crm_agent_changes_v16;

CREATE TABLE crm_agent_changes (
    id TEXT PRIMARY KEY NOT NULL,
    contact_id TEXT NOT NULL REFERENCES contacts(id),
    request_id TEXT NOT NULL,
    field TEXT NOT NULL CHECK(field IN ('stage','customer_level','primary_language','main_need','sample_status','notes','contact_method','product_interest')),
    previous_value TEXT NOT NULL,
    proposed_value TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('applied','pending','kept','accepted')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    detail_key TEXT,
    evidence_kind TEXT CHECK(evidence_kind IN ('research','inferred','unverified')),
    evidence_url TEXT,
    evidence_note TEXT
);

INSERT INTO crm_agent_changes(rowid,id,contact_id,request_id,field,previous_value,proposed_value,state,created_at)
SELECT rowid,id,contact_id,request_id,field,previous_value,proposed_value,state,created_at FROM crm_agent_changes_v16;

DROP TABLE crm_agent_changes_v16;
CREATE INDEX crm_agent_changes_state ON crm_agent_changes(state,created_at DESC,id);
CREATE INDEX crm_agent_changes_contact ON crm_agent_changes(contact_id, field, state);

CREATE VIEW crm_agent_change_versions AS
    SELECT a.*, a.rowid change_order, c.display_name, c.archived_at IS NOT NULL archived,
        CASE field
            WHEN 'stage' THEN c.stage
            WHEN 'customer_level' THEN c.customer_level
            WHEN 'primary_language' THEN c.primary_language
            WHEN 'main_need' THEN c.main_need
            WHEN 'sample_status' THEN c.sample_status
            WHEN 'notes' THEN c.notes
            WHEN 'contact_method' THEN COALESCE((
                SELECT json_object('kind',m.kind,'value',m.value,'label',m.label)
                FROM contact_methods m WHERE m.contact_id=c.id AND m.normalized=a.detail_key
                    AND m.kind=json_extract(a.proposed_value,'$.kind') ORDER BY m.position,m.id LIMIT 1
            ),'')
            WHEN 'product_interest' THEN COALESCE((
                SELECT json_object('id',i.id,'product_text',i.product_text,'intent_quantity',i.intent_quantity,
                    'quantity_is_estimate',i.quantity_is_estimate,'requirement',i.requirement)
                FROM customer_interests i WHERE i.contact_id=c.id AND i.product_text=a.detail_key COLLATE NOCASE
                ORDER BY i.position,i.id LIMIT 1
            ),'')
        END current_value
    FROM crm_agent_changes a JOIN contacts c ON c.id=a.contact_id;
