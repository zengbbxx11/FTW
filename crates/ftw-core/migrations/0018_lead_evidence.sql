CREATE TABLE lead_evidence (
    id TEXT PRIMARY KEY NOT NULL,
    lead_id TEXT NOT NULL REFERENCES lead_candidates(id),
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
    imported INTEGER NOT NULL DEFAULT 0 CHECK(imported IN (0,1))
);

CREATE INDEX lead_evidence_order ON lead_evidence(lead_id,created_at DESC,id);
CREATE UNIQUE INDEX lead_evidence_link ON lead_evidence(lead_id,url) WHERE imported=1;

INSERT OR IGNORE INTO lead_evidence(id,lead_id,subject,url,source,created_at,imported)
SELECT lower(hex(randomblob(16))), l.id, '来源网址', j.value, l.source, l.created_at, 1
FROM lead_candidates l, json_each(CASE WHEN json_valid(l.source_urls_json) THEN l.source_urls_json ELSE '[]' END) j
WHERE j.type='text' AND trim(j.value)<>'' AND l.source IN ('manual','agent');

CREATE TRIGGER notify_lead_evidence_insert AFTER INSERT ON lead_evidence
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='leads';
END;

CREATE TRIGGER notify_lead_evidence_update AFTER UPDATE ON lead_evidence
BEGIN
    UPDATE app_change_counters SET revision=revision+1 WHERE domain='leads';
END;
