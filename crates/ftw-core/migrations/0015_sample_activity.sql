ALTER TABLE activities ADD COLUMN sample_model TEXT NOT NULL DEFAULT '';
ALTER TABLE activities ADD COLUMN sample_quantity INTEGER CHECK(sample_quantity BETWEEN 1 AND 1000000);

CREATE INDEX activities_sample_contact_time ON activities(contact_id,occurred_at DESC,id)
WHERE event_type='sample';
