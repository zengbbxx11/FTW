ALTER TABLE activities ADD COLUMN sample_batch_ref TEXT NOT NULL DEFAULT '';
ALTER TABLE activities ADD COLUMN sample_progress TEXT NOT NULL DEFAULT '';

CREATE INDEX activities_sample_batch ON activities(contact_id,sample_batch_ref COLLATE NOCASE,occurred_at DESC,id)
WHERE event_type='sample';
