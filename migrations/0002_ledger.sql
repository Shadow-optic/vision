CREATE TABLE ledger_entries (
    seq          BIGSERIAL PRIMARY KEY,
    id           UUID NOT NULL UNIQUE,
    event_type   TEXT NOT NULL,
    payload      JSONB NOT NULL,
    payload_hash TEXT NOT NULL,
    prev_hash    TEXT NOT NULL,
    entry_hash   TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX ledger_entries_type_idx ON ledger_entries (event_type);

-- Append-only at the SQL layer. (Defense-in-depth; the application also never
-- issues UPDATE/DELETE. A managed-service superuser could still drop rules —
-- for litigation-grade anchoring, periodically notarize the tip hash externally.)
CREATE OR REPLACE RULE ledger_no_update AS ON UPDATE TO ledger_entries DO INSTEAD NOTHING;
CREATE OR REPLACE RULE ledger_no_delete AS ON DELETE TO ledger_entries DO INSTEAD NOTHING;
