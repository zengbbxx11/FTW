ALTER TABLE lead_candidates ADD COLUMN record_revision INTEGER NOT NULL DEFAULT 1 CHECK (record_revision >= 1);
