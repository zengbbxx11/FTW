DROP VIEW crm_agent_change_versions;

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
                SELECT CASE WHEN json_extract(a.proposed_value,'$.type')='correct_contact_method'
                    THEN json_object('kind',m.kind,'value',m.value,'label',m.label,'is_primary',m.is_primary,'is_verified',m.is_verified)
                    ELSE json_object('kind',m.kind,'value',m.value,'label',m.label) END
                FROM contact_methods m WHERE m.contact_id=c.id AND (
                   CASE WHEN json_extract(a.proposed_value,'$.type')='correct_contact_method' THEN m.id=a.detail_key
                   ELSE m.normalized=a.detail_key AND m.kind=json_extract(a.proposed_value,'$.kind') END)
                ORDER BY m.position,m.id LIMIT 1
            ),'')
            WHEN 'product_interest' THEN COALESCE((
                SELECT json_object('id',i.id,'product_text',i.product_text,'intent_quantity',i.intent_quantity,
                    'quantity_is_estimate',i.quantity_is_estimate,'requirement',i.requirement)
                FROM customer_interests i WHERE i.contact_id=c.id AND (
                    CASE WHEN json_extract(a.proposed_value,'$.type')='correct_product_interest' THEN i.id=a.detail_key
                    ELSE i.product_text=a.detail_key COLLATE NOCASE END)
                ORDER BY i.position,i.id LIMIT 1
            ),'')
        END current_value
    FROM crm_agent_changes a JOIN contacts c ON c.id=a.contact_id;
