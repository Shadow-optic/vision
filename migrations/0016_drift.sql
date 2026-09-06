-- B3: Doctrinal Drift Engine (vi-drift).
--
-- Bayesian online changepoint detection (Adams-MacKay BOCPD) over per-(court,
-- clause) outcome-signal time series built from ingested opinions and
-- constitution-screen hits.
--
-- Every signal is a machine-derived proxy read off opinion text by a
-- transparent lexicon (vi_drift::lexicon). These rows are LEADS for counsel,
-- never findings: no lexicon hit means no row, and every row carries
-- machine_derived = TRUE so it can never be mistaken for a reviewed fact.

CREATE TABLE drift_observations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Source court id (e.g. CourtListener 'cand'), falling back to the
    -- resolved forum code when the source never named a court.
    court_id        TEXT NOT NULL,
    clause_id       TEXT NOT NULL,
    observed_at     DATE NOT NULL,
    -- Outcome proxy in [0,1]: 1 = relief granted to the movant/defendant,
    -- 0 = relief denied / affirmed against. Lexicon-weighted; see vi-drift.
    signal          DOUBLE PRECISION NOT NULL,
    -- Stable source pointer ('cl-opinion:<id>' or 'opinion:<uuid>'); with
    -- (court_id, clause_id) it is the idempotence key for re-ingestion.
    source_ref      TEXT NOT NULL,
    machine_derived BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (court_id, clause_id, source_ref)
);
COMMENT ON TABLE drift_observations IS
    'Machine-derived outcome-signal series per (court, clause). Advisory only; '
    'a row exists only where the transparent lexicon matched opinion text.';
COMMENT ON COLUMN drift_observations.machine_derived IS
    'Always TRUE today: every signal comes from the vi-drift lexicon, not from '
    'counsel review. Kept explicit so a future human-coded signal is marked FALSE.';

CREATE INDEX drift_observations_series_idx
    ON drift_observations (court_id, clause_id, observed_at);

CREATE TABLE drift_runs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    court_id    TEXT NOT NULL,
    clause_id   TEXT NOT NULL,
    -- Run-length posterior summary + run parameters: hazard (expected run
    -- length), detection threshold, observation count, final run-length
    -- distribution (truncated), and how many changepoints were recorded.
    run_length  JSONB NOT NULL,
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX drift_runs_series_idx ON drift_runs (court_id, clause_id, computed_at);

CREATE TABLE drift_changepoints (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    court_id    TEXT NOT NULL,
    clause_id   TEXT NOT NULL,
    at_date     DATE NOT NULL,
    -- Posterior probability that the previously dominant run ended here
    -- (1 - P(growth of the prior MAP run)). Data-dependent; see vi-drift docs.
    posterior   DOUBLE PRECISION NOT NULL,
    -- Context: observation window around the detection (indices, dates,
    -- signals) and the drift_run id that produced it.
    "window"    JSONB NOT NULL DEFAULT '{}',
    status      TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending','substantiated','rejected')),
    computed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
COMMENT ON TABLE drift_changepoints IS
    'Machine-detected doctrinal drift candidates. Pending until counsel review; '
    'a changepoint is a lead about shifting outcomes, never an accusation.';
CREATE INDEX drift_changepoints_series_idx
    ON drift_changepoints (court_id, clause_id, at_date);
CREATE INDEX drift_changepoints_posterior_idx
    ON drift_changepoints (posterior);
