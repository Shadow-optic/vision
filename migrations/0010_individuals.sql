-- Naming individuals correctly, and admitting when we cannot.
--
-- Live feeds put panels in the judge field: "Mathias, DeBoer, Kenworthy" is
-- three judges. Stored whole, it became a fourth official who does not exist,
-- while the three real ones accumulated nothing. Splitting it is the whole
-- point of an individual accountability doctrine, so the pipeline now splits.
--
-- Some fields cannot be split. "Smith, John" is either one judge written
-- surname-first or two judges, and the text does not say which. Guessing would
-- put one official's conduct on another official's record — the same wrong this
-- system exists to answer. So the unreadable ones land in a queue for a human
-- instead of being resolved into an identity.

CREATE TABLE unresolved_officials (
    unresolved_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id       UUID NOT NULL REFERENCES court_cases(case_id) ON DELETE CASCADE,
    role_in_case  TEXT NOT NULL,
    -- Exactly what the source published, unedited.
    raw_value     TEXT NOT NULL,
    -- 'ambiguous' = names several individuals, boundaries unreadable.
    -- 'collective' = names the court as a body, not a person.
    reason_kind   TEXT NOT NULL CHECK (reason_kind IN ('ambiguous', 'collective')),
    reason        TEXT NOT NULL,
    resolved_at   TIMESTAMPTZ,
    resolved_by   TEXT,
    resolution    TEXT,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (case_id, role_in_case, raw_value)
);
CREATE INDEX unresolved_officials_open_idx
    ON unresolved_officials (reason_kind) WHERE resolved_at IS NULL;

COMMENT ON TABLE unresolved_officials IS
    'Judge and counsel fields that could not be read as individuals without '
    'guessing. Nothing here is attributed to anyone. A collective entry needs '
    'no action; an ambiguous one is a question only a human can close.';

-- ===== Keep the source court id out of the jurisdiction column =====

-- Ingestion had nowhere to put a court id it could not place, so it wrote the
-- id into `jurisdiction` and a case came out claiming to sit in the forum
-- "txctapp6". A column named jurisdiction should hold a forum or say it does
-- not know.
ALTER TABLE court_cases ADD COLUMN source_court_id TEXT;
CREATE INDEX court_cases_source_court_idx ON court_cases (source_court_id);

COMMENT ON COLUMN court_cases.source_court_id IS
    'The court id as the source published it. Kept verbatim so an unplaced '
    'court is a lookup that can be fixed later, not a fabricated forum.';

UPDATE court_cases c
   SET source_court_id = c.jurisdiction,
       jurisdiction    = 'unknown'
 WHERE NOT EXISTS (SELECT 1 FROM court_registry r WHERE r.jurisdiction = c.jurisdiction)
   AND c.jurisdiction <> 'unknown'
   AND c.jurisdiction = lower(c.jurisdiction)
   AND length(c.jurisdiction) > 2;

-- Panel strings already resolved into a single fake "official" are not
-- salvageable in place: the individuals behind them have to be re-derived from
-- the record. Drop the links and identities that a panel string produced so the
-- pipeline can rebuild them, and leave anything a human has touched alone.
DELETE FROM actor_case_links l
 USING accountability_actors a
 WHERE l.actor_id = a.actor_id
   AND (a.display_name LIKE '%;%' OR a.display_name LIKE '%,%,%'
        OR a.display_name ILIKE 'form field%');

DELETE FROM accountability_actors a
 WHERE (a.display_name LIKE '%;%' OR a.display_name LIKE '%,%,%'
        OR a.display_name ILIKE 'form field%')
   AND a.prosecutor_id IS NULL
   AND NOT EXISTS (SELECT 1 FROM actor_case_links l WHERE l.actor_id = a.actor_id)
   AND NOT EXISTS (SELECT 1 FROM publication_approvals p WHERE p.actor_id = a.actor_id);
