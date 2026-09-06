-- Live ingestion from public feeds.
--
-- Three things the fixture-only path never needed:
--   1. Provenance. A record pulled from a public feed may carry only a snippet
--      of the opinion text. Storing a snippet in a column named `full_text`
--      without saying so would misrepresent the record, so completeness is
--      recorded per row and every row keeps its source reference.
--   2. Idempotence. A feed is polled forever. Re-reading the same opinion must
--      not create a second row, so opinions get a dedupe key.
--   3. Jurisdiction. Feeds identify a court, not a forum. The court registry
--      below is the lookup that turns "cand" into California / federal
--      district, which is what constitutional screening needs.
--
-- Findings and flags also gain an actor link. They were keyed to
-- `prosecutor_id`, so an official who is not a seeded prosecutor — a judge from
-- a live opinion, for instance — could never accumulate a record. Individual
-- accountability cannot depend on which table an official happens to sit in.

-- ===== Opinion provenance and idempotence =====

-- Idempotence already exists: 0005 made (case_id, md5(full_text)) unique, which
-- is the right key for identical text arriving twice. What was missing is the
-- provenance of the text itself.
ALTER TABLE court_opinions
    ADD COLUMN source_url TEXT,
    ADD COLUMN source_ref TEXT,
    ADD COLUMN text_completeness TEXT NOT NULL DEFAULT 'full'
        CHECK (text_completeness IN ('full', 'snippet', 'summary')),
    ADD COLUMN ingested_at TIMESTAMPTZ NOT NULL DEFAULT now();

COMMENT ON COLUMN court_opinions.text_completeness IS
    'full = the whole opinion text; snippet/summary = a partial extract from a '
    'public feed. Derived analysis must not treat a snippet as exhaustive: an '
    'absent mention in a snippet is absence of evidence, not evidence of absence.';
COMMENT ON COLUMN court_opinions.source_ref IS
    'Stable identifier at the source (e.g. cl-opinion:11435101). When complete '
    'text later arrives for the same source record, the partial row it '
    'supersedes is removed rather than left to double-count.';

CREATE INDEX court_opinions_source_ref_idx ON court_opinions (source_ref);
CREATE INDEX court_opinions_completeness_idx ON court_opinions (text_completeness);

-- ===== Court registry (public courts feed) =====

CREATE TABLE court_registry (
    court_id        TEXT PRIMARY KEY,          -- source court id, e.g. 'cand'
    full_name       TEXT NOT NULL,
    short_name      TEXT,
    citation_string TEXT,
    -- The source's own classification (F, FD, FB, FS, S, SA, ST, SS, ...).
    source_class    TEXT,
    -- Derived VisionInjustice forum code: a state/territory code, or 'US'.
    jurisdiction    TEXT,
    -- Derived level, matching the vocabulary the screening engine expects.
    court_level     TEXT,
    -- How the jurisdiction was derived, so a wrong mapping is auditable.
    mapping_method  TEXT,
    in_use          BOOLEAN NOT NULL DEFAULT FALSE,
    parent_court    TEXT,
    start_date      DATE,
    end_date        DATE,
    source_url      TEXT,
    raw             JSONB NOT NULL DEFAULT '{}',
    refreshed_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX court_registry_jurisdiction_idx ON court_registry (jurisdiction);
CREATE INDEX court_registry_in_use_idx ON court_registry (in_use);

COMMENT ON TABLE court_registry IS
    'Public registry of courts, refreshed from the courts feed. Maps a source '
    'court id onto a forum code and court level so ingested records can be '
    'screened. mapping_method records how each row was derived.';

-- ===== Feed bookkeeping =====

ALTER TABLE ingest_cursors
    ADD COLUMN feed_kind            TEXT,
    ADD COLUMN label                TEXT,
    ADD COLUMN last_started_at      TIMESTAMPTZ,
    ADD COLUMN last_ok_at           TIMESTAMPTZ,
    ADD COLUMN consecutive_failures INT NOT NULL DEFAULT 0,
    ADD COLUMN last_cases           INT NOT NULL DEFAULT 0,
    ADD COLUMN last_opinions        INT NOT NULL DEFAULT 0,
    ADD COLUMN last_skipped         INT NOT NULL DEFAULT 0,
    ADD COLUMN total_cases          BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN total_opinions       BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN total_skipped        BIGINT NOT NULL DEFAULT 0;

COMMENT ON COLUMN ingest_cursors.total_cases IS
    'Cumulative count of records this feed was the first to bring in. A live '
    'feed re-reads its own head forever, so counting every write would inflate '
    'this into meaninglessness.';
COMMENT ON COLUMN ingest_cursors.last_skipped IS
    'Records the source offered and ingestion refused: sealed, juvenile, '
    'expunged, blocked, or empty. Published on the sources page.';

-- ===== Actor-level linkage for findings and flags =====

ALTER TABLE constitutional_findings
    ADD COLUMN actor_id UUID REFERENCES accountability_actors(actor_id) ON DELETE SET NULL;
CREATE INDEX cf_actor_idx ON constitutional_findings (actor_id);

ALTER TABLE abuse_flags
    ADD COLUMN actor_id UUID REFERENCES accountability_actors(actor_id) ON DELETE SET NULL;
CREATE INDEX abuse_flags_actor_idx ON abuse_flags (actor_id);

COMMENT ON COLUMN constitutional_findings.actor_id IS
    'The individual this finding concerns. Set for any role — judge, officer, '
    'expert — not just seeded prosecutors. Review status still gates publication.';

-- ===== Post-ingest pipeline runs =====

CREATE TABLE pipeline_runs (
    run_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id      UUID NOT NULL REFERENCES court_cases(case_id) ON DELETE CASCADE,
    trigger      TEXT NOT NULL DEFAULT 'ingest',
    status       TEXT NOT NULL DEFAULT 'ok'
                 CHECK (status IN ('ok', 'partial', 'failed', 'skipped')),
    stages       JSONB NOT NULL DEFAULT '[]',
    screen_id    UUID,
    screen_hits  INT NOT NULL DEFAULT 0,
    flags_fired  INT NOT NULL DEFAULT 0,
    expected_items INT NOT NULL DEFAULT 0,
    evidence_gaps  INT NOT NULL DEFAULT 0,
    actors_linked  INT NOT NULL DEFAULT 0,
    error        TEXT,
    started_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at  TIMESTAMPTZ
);
CREATE INDEX pipeline_runs_case_idx ON pipeline_runs (case_id, started_at DESC);
CREATE INDEX pipeline_runs_status_idx ON pipeline_runs (status);

COMMENT ON TABLE pipeline_runs IS
    'One row per case processed after ingestion. Everything the pipeline '
    'produces is pending by construction: screens, leads, flags, and links. '
    'Nothing here publishes anything about an individual.';
