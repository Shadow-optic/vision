-- Core case-law, prosecutor, tactics, TrustScript, simulation schema.
CREATE TABLE prosecutors (
    prosecutor_id UUID PRIMARY KEY,
    name          TEXT NOT NULL,
    office        TEXT NOT NULL,
    jurisdiction  TEXT NOT NULL,
    metadata      JSONB NOT NULL DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE court_cases (
    case_id             UUID PRIMARY KEY,
    docket_number       TEXT UNIQUE,
    jurisdiction        TEXT NOT NULL,
    court_level         TEXT,
    case_type           TEXT,
    charge_category     TEXT,
    charges             TEXT[],
    prosecutor_id       UUID REFERENCES prosecutors(prosecutor_id),
    judge_id            UUID,
    -- Public-record display name when known (opinions, dockets). Not a PII store.
    judge               TEXT,
    defense_attorney_id UUID,
    -- Restricted: populated only where lawfully sourced; pseudonymized in public views.
    defendant_race      TEXT,
    evidence_strength   TEXT,  -- attorney-assessed or derived: strong/mixed/weak
    incident_location_lat DOUBLE PRECISION,
    incident_location_lng DOUBLE PRECISION,
    court_location_lat  DOUBLE PRECISION,
    court_location_lng  DOUBLE PRECISION,
    incident_h3_cell    TEXT,
    court_h3_cell       TEXT,
    resolution          INT NOT NULL DEFAULT 8,
    filing_date         TIMESTAMPTZ,
    disposition_date    TIMESTAMPTZ,
    outcome             TEXT,  -- conviction/acquittal/dismissal/pending
    plea_offered        BOOLEAN,
    plea_accepted       BOOLEAN,
    plea_offer_months   INT,
    sentence_months     INT,
    sentence_type       TEXT,
    source_url          TEXT,
    raw_data            JSONB NOT NULL DEFAULT '{}',
    hash                TEXT,
    inserted_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX court_cases_prosecutor_idx ON court_cases (prosecutor_id);
CREATE INDEX court_cases_outcome_idx    ON court_cases (outcome);
CREATE INDEX court_cases_h3_idx         ON court_cases (court_h3_cell);
CREATE INDEX court_cases_judge_idx      ON court_cases (judge);

CREATE TABLE case_h3_cells (
    case_id    UUID REFERENCES court_cases(case_id) ON DELETE CASCADE,
    h3_cell    TEXT NOT NULL,
    resolution INT  NOT NULL,
    cell_type  TEXT NOT NULL CHECK (cell_type IN ('incident','court')),
    PRIMARY KEY (case_id, h3_cell, resolution, cell_type)
);

CREATE TABLE court_opinions (
    opinion_id  UUID PRIMARY KEY,
    case_id     UUID REFERENCES court_cases(case_id) ON DELETE CASCADE,
    court_level TEXT,
    judge       TEXT,
    citation    TEXT,
    date_issued DATE,
    full_text   TEXT NOT NULL,
    tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', full_text)) STORED
);
CREATE INDEX court_opinions_tsv_idx ON court_opinions USING GIN (tsv);

CREATE TABLE tactics (
    tactic_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    description  TEXT NOT NULL,
    category     TEXT NOT NULL,   -- prosecution/defense/judicial
    success_rate REAL,
    data_points  INT  NOT NULL DEFAULT 0,
    source_url   TEXT
);

CREATE TABLE abuse_rules (
    rule_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL UNIQUE,
    source     TEXT NOT NULL,      -- TrustScript source; validated on insert
    enabled    BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE abuse_flags (
    flag_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id       UUID REFERENCES court_cases(case_id) ON DELETE CASCADE,
    rule_id       UUID REFERENCES abuse_rules(rule_id),
    prosecutor_id UUID REFERENCES prosecutors(prosecutor_id),
    office        TEXT,
    label         TEXT NOT NULL,
    severity      TEXT NOT NULL,
    explanation   JSONB NOT NULL DEFAULT '[]',
    -- Flags are LEADS. Nothing is published until review_status = 'substantiated'
    -- by the Evidence Review Committee. Publishing an unreviewed automated flag
    -- against a named individual is a defamation risk and a charter violation.
    review_status TEXT NOT NULL DEFAULT 'pending'
                  CHECK (review_status IN ('pending','substantiated','rejected')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX abuse_flags_office_idx ON abuse_flags (office);
CREATE INDEX abuse_flags_status_idx ON abuse_flags (review_status);

CREATE TABLE simulations (
    sim_id     UUID PRIMARY KEY,
    seed       TEXT NOT NULL,      -- hex/u64; full reproducibility guarantee
    trials     INT  NOT NULL,
    params     JSONB NOT NULL,
    result     JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
