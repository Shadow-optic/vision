-- B4: Structural Capture Graph (vi-capture).
--
-- Repeat-appearance concentration: do the same authoring judges pair with the
-- same outcome signals (or offices) far beyond what a degree-preserving
-- randomization of the same data produces?
--
-- Authorship comes from court_opinions.judge, which vi-ingest already
-- populates from CourtListener's author_str (authenticated API) and judge
-- (search) fields, so no schema change to opinions is required. Edges exist
-- only where an opinion names an author AND the transparent vi-drift lexicon
-- matched its text. Everything here is machine-derived and pending review.

CREATE TABLE capture_edges (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    judge_name     TEXT NOT NULL,
    court_id       TEXT NOT NULL,
    -- Prosecuting office when the case record carries one; usually NULL for
    -- feed-sourced opinions. Absence is stated, never guessed.
    office         TEXT,
    outcome_signal DOUBLE PRECISION NOT NULL,
    case_id        UUID,
    observed_at    DATE NOT NULL,
    source_ref     TEXT NOT NULL,
    machine_derived BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (judge_name, court_id, source_ref)
);
COMMENT ON TABLE capture_edges IS
    'Machine-derived judge x court x outcome-signal appearances rebuilt from '
    'ingested opinions by vi_capture::rebuild_edges. Advisory leads only.';
CREATE INDEX capture_edges_judge_idx ON capture_edges (judge_name, court_id);
CREATE INDEX capture_edges_office_idx ON capture_edges (office) WHERE office IS NOT NULL;

CREATE TABLE capture_metrics (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- 'judge' (entity_key = 'Name @ court_id') or 'office' (entity_key = office).
    entity_kind TEXT NOT NULL CHECK (entity_kind IN ('judge','office')),
    entity_key  TEXT NOT NULL,
    appearances INT NOT NULL,
    -- Outcome concentration over {relief, mixed, adverse} buckets.
    gini        DOUBLE PRECISION NOT NULL,
    entropy     DOUBLE PRECISION NOT NULL,
    -- Degree-preserving Monte Carlo null (seeded rand_chacha): mean Gini and
    -- empirical p (plus-one corrected) for the observed concentration.
    null_mean   DOUBLE PRECISION NOT NULL,
    null_p      DOUBLE PRECISION NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending','substantiated','rejected'))
);
COMMENT ON TABLE capture_metrics IS
    'Machine-derived concentration metrics with Monte Carlo null p-values. '
    'Pending until counsel review; low p is a lead, not a finding of capture.';
CREATE INDEX capture_metrics_p_idx ON capture_metrics (null_p);
