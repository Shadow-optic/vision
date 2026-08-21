-- Native Constitution / Bill of Rights engine.
-- Corpus text and holdings are upserted from vi-constitution::sync_native after migrate.

CREATE TABLE jurisdiction_circuits (
    code       TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL
               CHECK (kind IN ('state','district','territory','federal')),
    circuit    TEXT NOT NULL,
    selectable BOOLEAN NOT NULL DEFAULT true,
    sort_order INT NOT NULL
);
CREATE INDEX jurisdiction_circuits_circuit_idx ON jurisdiction_circuits (circuit);
CREATE INDEX jurisdiction_circuits_kind_idx ON jurisdiction_circuits (kind);

CREATE TABLE constitution_provisions (
    provision_id    TEXT PRIMARY KEY,
    kind            TEXT NOT NULL
                    CHECK (kind IN ('preamble','article','section','amendment','clause')),
    parent_id       TEXT REFERENCES constitution_provisions(provision_id),
    citation_label  TEXT NOT NULL,
    sort_order      INT NOT NULL,
    body            TEXT NOT NULL,
    tsv tsvector GENERATED ALWAYS AS (
        to_tsvector('english', coalesce(citation_label,'') || ' ' || body)
    ) STORED
);
CREATE INDEX constitution_provisions_tsv_idx ON constitution_provisions USING GIN (tsv);
CREATE INDEX constitution_provisions_kind_idx ON constitution_provisions (kind);

CREATE TABLE constitution_holdings (
    holding_id      TEXT PRIMARY KEY,
    citation        TEXT NOT NULL,
    year            INT NOT NULL,
    court_kind      TEXT NOT NULL
                    CHECK (court_kind IN ('scotus','circuit','state')),
    court_id        TEXT NOT NULL,
    authority       TEXT NOT NULL
                    CHECK (authority IN (
                        'controlling','circuit_binding','persuasive','split','overruled'
                    )),
    rule_statement  TEXT NOT NULL,
    superseded_by   TEXT,
    snapshot_id     TEXT NOT NULL
);

CREATE TABLE constitution_holding_clauses (
    holding_id TEXT NOT NULL REFERENCES constitution_holdings(holding_id) ON DELETE CASCADE,
    clause_id  TEXT NOT NULL,
    PRIMARY KEY (holding_id, clause_id)
);
CREATE INDEX constitution_holding_clauses_clause_idx ON constitution_holding_clauses (clause_id);

CREATE TABLE constitution_splits (
    split_id         TEXT PRIMARY KEY,
    clause_id        TEXT NOT NULL,
    question         TEXT NOT NULL,
    side_a_circuits  TEXT[] NOT NULL,
    side_a_view      TEXT NOT NULL,
    side_b_circuits  TEXT[] NOT NULL,
    side_b_view      TEXT NOT NULL,
    notes            TEXT,
    snapshot_id      TEXT NOT NULL
);

CREATE TABLE constitution_state_analogs (
    code            TEXT NOT NULL REFERENCES jurisdiction_circuits(code),
    clause_id       TEXT NOT NULL,
    state_citation  TEXT NOT NULL,
    relation        TEXT NOT NULL
                    CHECK (relation IN ('independent','lockstep','unspecified')),
    more_protective BOOLEAN,
    notes           TEXT,
    PRIMARY KEY (code, clause_id)
);
CREATE INDEX constitution_state_analogs_clause_idx ON constitution_state_analogs (clause_id);

CREATE TABLE constitution_screens (
    screen_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id       UUID REFERENCES court_cases(case_id) ON DELETE CASCADE,
    jurisdiction  TEXT NOT NULL,
    circuit       TEXT,
    snapshot_id   TEXT NOT NULL,
    corpus_hash   TEXT NOT NULL,
    hit_count     INT NOT NULL DEFAULT 0,
    report        JSONB NOT NULL,
    review_status TEXT NOT NULL DEFAULT 'pending'
                  CHECK (review_status IN ('pending','substantiated','rejected')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX constitution_screens_case_idx ON constitution_screens (case_id);
CREATE INDEX constitution_screens_jur_idx ON constitution_screens (jurisdiction);

CREATE TABLE constitution_screen_hits (
    hit_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    screen_id   UUID NOT NULL REFERENCES constitution_screens(screen_id) ON DELETE CASCADE,
    clause_id   TEXT NOT NULL,
    authority   TEXT NOT NULL,
    citation    TEXT,
    severity    TEXT NOT NULL,
    matched     TEXT[] NOT NULL,
    resolution  JSONB NOT NULL
);
CREATE INDEX constitution_screen_hits_screen_idx ON constitution_screen_hits (screen_id);

CREATE TABLE constitution_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Advisory TrustScript rule: derived constitution.* features computed in Rust.
INSERT INTO abuse_rules (name, source) VALUES
(
    'sixth-amendment-trial-pressure',
    'when constitution.amend_06.trial_right_pressure == true then flag "Sixth Amendment trial-right pressure — human review required" severity high'
)
ON CONFLICT (name) DO NOTHING;
