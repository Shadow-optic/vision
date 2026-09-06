-- Counsel review gates and full-engine pipeline wiring.
--
-- The audit found four places where the loop "ingest -> flag -> counsel
-- review -> publication -> tracker" was cut: abuse flags, constitution
-- screens, and legal-action packages had review_status / status columns no
-- route could move, and the unresolved-officials queue was insert-only.
-- This migration adds the review bookkeeping those gates need and the
-- tables the extended pipeline stages write to.

-- ===== A1: abuse-flag review =============================================

ALTER TABLE abuse_flags ADD COLUMN reviewed_at TIMESTAMPTZ;
ALTER TABLE abuse_flags ADD COLUMN review_notes TEXT;

COMMENT ON COLUMN abuse_flags.reviewed_at IS
    'Set when counsel substantiates or rejects the flag. A pending flag has '
    'never been reviewed; nothing publishes from it.';

-- ===== A2: package transition notes ======================================

ALTER TABLE legal_action_packages ADD COLUMN review_notes TEXT;

-- ===== A3: constitution screen review + unresolved close-out =============

ALTER TABLE constitution_screens ADD COLUMN reviewed_at TIMESTAMPTZ;
ALTER TABLE constitution_screens ADD COLUMN review_notes TEXT;
ALTER TABLE unresolved_officials ADD COLUMN resolution_notes TEXT;

-- ===== A4: full-engine pipeline artifacts ================================

-- Tactic occurrence matching: a tactic catalog signal observed in a case's
-- public text. Occurrences are LEADS (pending) — an occurrence records that
-- the record mentions the doctrine, not that anyone used the tactic.
CREATE TABLE tactic_occurrences (
    occurrence_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id       UUID NOT NULL REFERENCES court_cases(case_id) ON DELETE CASCADE,
    tactic_id     UUID NOT NULL REFERENCES tactics(tactic_id) ON DELETE CASCADE,
    matched_term  TEXT NOT NULL,
    match_source  TEXT NOT NULL CHECK (match_source IN ('opinion_text', 'charges')),
    review_status TEXT NOT NULL DEFAULT 'pending'
                  CHECK (review_status IN ('pending','substantiated','rejected')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (case_id, tactic_id, matched_term, match_source)
);
CREATE INDEX tactic_occurrences_case_idx ON tactic_occurrences (case_id);
CREATE INDEX tactic_occurrences_status_idx ON tactic_occurrences (review_status);

-- Trial-penalty accumulation: which cases actually carried disposition
-- fields into the office distributions. Cases without those fields are
-- counted as skipped by the pipeline, never silently dropped.
CREATE TABLE trial_penalty_observations (
    case_id           UUID PRIMARY KEY REFERENCES court_cases(case_id) ON DELETE CASCADE,
    office            TEXT,
    jurisdiction      TEXT NOT NULL,
    plea_offered      BOOLEAN,
    plea_accepted     BOOLEAN,
    outcome           TEXT,
    plea_offer_months INT,
    sentence_months   INT,
    observed_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Monell fingerprint refresh: latest computed fingerprint per office, so a
-- refresh is durable instead of a warm-up nobody can audit.
CREATE TABLE monell_fingerprints (
    office       TEXT PRIMARY KEY,
    jurisdiction TEXT,
    fingerprint  JSONB NOT NULL,
    computed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Office statistics the zero-day sim calibrates from. The pipeline refreshes
-- one row per office as cases accumulate; /simulate/from-case prefers these
-- stored aggregates and only falls back to a live computation when an office
-- has never been refreshed.
CREATE TABLE office_sim_stats (
    office               TEXT PRIMARY KEY,
    jurisdiction         TEXT,
    cases_with_outcome   INT NOT NULL DEFAULT 0,
    conviction_rate      DOUBLE PRECISION,
    mean_plea_months     DOUBLE PRECISION,
    mean_trial_months    DOUBLE PRECISION,
    -- Plea-offer vs sentence correlation for the office (vi-correlation);
    -- NULL when fewer than 4 paired observations exist. Absence is reported,
    -- never filled in.
    plea_sentence_r      DOUBLE PRECISION,
    plea_sentence_n      INT NOT NULL DEFAULT 0,
    computed_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One row per pipeline batch: per-stage processed / skipped-no-data / failed
-- counts across the cases the batch walked. A stage that had nothing to work
-- with shows up here as skipped, not as silence.
CREATE TABLE pipeline_reports (
    report_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trigger         TEXT NOT NULL,
    cases_processed INT NOT NULL,
    stage_counts    JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
