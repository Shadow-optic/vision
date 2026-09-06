-- B1: Transparency Proof Log (vi-transparency).
-- Certificate-transparency-style tamper evidence for the published dataset.
-- Each snapshot is a Merkle root over the canonical hashes of every published
-- row (wall entries, statute catalog, substantiated findings, referred
-- packages, ledger events), chained to the previous snapshot root and to the
-- ledger head hash at capture time. Snapshots are audit artifacts; they never
-- change what is published and carry no accusatory content themselves.

CREATE TABLE transparency_snapshots (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    merkle_root  BYTEA NOT NULL,
    tree_size    INT  NOT NULL,
    -- {"wall_entries": n, "statutes": n, "findings": n, "referrals": n,
    --  "ledger_events": n, "chain": 2}
    table_counts JSONB NOT NULL DEFAULT '{}',
    -- Previous snapshot's merkle_root (NULL for the first snapshot). The
    -- previous root is also committed inside the tree as a chain leaf, so
    -- merkle_root alone authenticates the full history.
    prev_root    BYTEA,
    -- ledger_entries.seq of the ledger head at capture time. The head's
    -- entry_hash is committed inside the tree as a chain leaf.
    ledger_seq   BIGINT
);
CREATE INDEX transparency_snapshots_created_idx
    ON transparency_snapshots (created_at DESC);

-- Leaf inventory per snapshot: lets inclusion proofs be served for a past
-- snapshot without trusting the (mutable) source tables to be unchanged.
CREATE TABLE transparency_leaves (
    snapshot_id UUID NOT NULL REFERENCES transparency_snapshots(id) ON DELETE CASCADE,
    leaf_index  INT  NOT NULL,
    table_name  TEXT NOT NULL,
    row_id      TEXT NOT NULL,
    leaf_hash   BYTEA NOT NULL,
    PRIMARY KEY (snapshot_id, leaf_index)
);
CREATE INDEX transparency_leaves_row_idx
    ON transparency_leaves (table_name, row_id);
