-- Cursor checkpoints for poll-based ingest (one row per source name).
CREATE TABLE ingest_cursors (
    source         TEXT PRIMARY KEY,
    next_url       TEXT,
    last_polled_at TIMESTAMPTZ,
    last_count     INT NOT NULL DEFAULT 0,
    last_error     TEXT
);

-- Tactic catalog matching: a stable signal key used to recompute rates from
-- public-record tables (findings, flags, plea/trial outcomes).
ALTER TABLE tactics ADD COLUMN signal TEXT;
CREATE UNIQUE INDEX tactics_description_uidx ON tactics (lower(description));
CREATE INDEX tactics_signal_idx ON tactics (signal);
CREATE INDEX tactics_category_idx ON tactics (category);

-- Avoid duplicate opinion re-ingest of the same text for a case.
CREATE UNIQUE INDEX court_opinions_case_text_idx
    ON court_opinions (case_id, md5(full_text));

-- Give the charge-stacking signal something to count in the demo fixture.
UPDATE court_cases
   SET charges = ARRAY['possession of a controlled substance', 'possession with intent to distribute']
 WHERE case_id = '22222222-2222-2222-2222-222222222222'
   AND (charges IS NULL OR cardinality(charges) = 0);

-- Public-record tactic catalog. No named individuals. Citations are doctrines
-- and canonical cases, not accusations.
INSERT INTO tactics (tactic_id, description, category, signal, source_url, data_points, success_rate) VALUES
(
    'aaaaaaaa-0000-4000-8000-000000000001',
    'Brady withholding of material exculpatory or impeachment evidence',
    'prosecution',
    'brady',
    'https://www.oyez.org/cases/1962/490',
    0, NULL
),
(
    'aaaaaaaa-0000-4000-8000-000000000002',
    'Giglio non-disclosure of witness credibility deals or benefits',
    'prosecution',
    'giglio',
    'https://www.oyez.org/cases/1971/70-29',
    0, NULL
),
(
    'aaaaaaaa-0000-4000-8000-000000000003',
    'Batson-pattern jury strikes on the basis of race or sex',
    'prosecution',
    'batson',
    'https://www.oyez.org/cases/1985/84-6263',
    0, NULL
),
(
    'aaaaaaaa-0000-4000-8000-000000000004',
    'Late or incomplete discovery production (sandbagging)',
    'prosecution',
    'discovery',
    'https://www.law.cornell.edu/wex/brady_rule',
    0, NULL
),
(
    'aaaaaaaa-0000-4000-8000-000000000005',
    'Informant or cooperator benefit concealment',
    'prosecution',
    'informant',
    'https://www.law.cornell.edu/wex/giglio_material',
    0, NULL
),
(
    'aaaaaaaa-0000-4000-8000-000000000006',
    'Trial-penalty leverage: sentence premium after rejected plea',
    'prosecution',
    'trial_penalty',
    'https://www.law.cornell.edu/supremecourt/text/395/238',
    0, NULL
),
(
    'aaaaaaaa-0000-4000-8000-000000000007',
    'Charge stacking to inflate plea pressure',
    'prosecution',
    'charge_stack',
    'https://www.law.cornell.edu/wex/charge_stacking',
    0, NULL
)
ON CONFLICT DO NOTHING;
