ALTER TABLE lead_evidence RENAME TO lead_evidence_v19;

CREATE TABLE lead_evidence (
    id TEXT PRIMARY KEY NOT NULL,
    lead_id TEXT REFERENCES lead_candidates(id),
    contact_id TEXT REFERENCES contacts(id),
    subject TEXT NOT NULL,
    url TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL CHECK (source IN ('manual','agent')),
    actor_id TEXT,
    request_id TEXT,
    observed_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','verified','rejected')),
    reviewed_at INTEGER,
    reviewed_by TEXT,
    revision INTEGER NOT NULL DEFAULT 1,
    imported INTEGER NOT NULL DEFAULT 0 CHECK(imported IN (0,1)),
    CHECK((lead_id IS NULL) <> (contact_id IS NULL))
);
INSERT INTO lead_evidence(rowid,id,lead_id,subject,url,note,source,actor_id,request_id,observed_at,created_at,status,reviewed_at,reviewed_by,revision,imported)
SELECT rowid,id,lead_id,subject,url,note,source,actor_id,request_id,observed_at,created_at,status,reviewed_at,reviewed_by,revision,imported
FROM lead_evidence_v19;
DROP TABLE lead_evidence_v19;
CREATE INDEX lead_evidence_order ON lead_evidence(lead_id,created_at DESC,id);
CREATE INDEX contact_evidence_order ON lead_evidence(contact_id,created_at DESC,id);
CREATE UNIQUE INDEX lead_evidence_link ON lead_evidence(lead_id,url) WHERE imported=1;

CREATE TRIGGER notify_evidence_insert AFTER INSERT ON lead_evidence
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain=CASE WHEN new.contact_id IS NULL THEN 'leads' ELSE 'contacts' END;
END;
CREATE TRIGGER notify_evidence_update AFTER UPDATE ON lead_evidence
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain=CASE WHEN new.contact_id IS NULL THEN 'leads' ELSE 'contacts' END;
END;
