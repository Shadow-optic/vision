-- Reckoning Engine: named-individual accountability from public records.
-- Nothing is public until (1) at least one substantiated finding and
-- (2) an Evidence Review Committee publication approval. No photos, no
-- home addresses, no sealed/juvenile/expunged records.

CREATE TABLE accountability_actors (
    actor_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role            TEXT NOT NULL
                    CHECK (role IN ('prosecutor','officer','judge','expert','other')),
    display_name    TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    office          TEXT,
    jurisdiction    TEXT NOT NULL,
    bar_number      TEXT,
    badge_number    TEXT,
    prosecutor_id   UUID REFERENCES prosecutors(prosecutor_id) ON DELETE SET NULL,
    fingerprint     TEXT NOT NULL,
    employment_start DATE,
    employment_end   DATE,
    metadata        JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX actors_fingerprint_uidx ON accountability_actors (fingerprint);
CREATE INDEX actors_role_jur_idx ON accountability_actors (role, jurisdiction);
CREATE INDEX actors_norm_idx ON accountability_actors (normalized_name);
CREATE INDEX actors_bar_idx ON accountability_actors (bar_number);
CREATE INDEX actors_badge_idx ON accountability_actors (badge_number);
CREATE INDEX actors_prosecutor_idx ON accountability_actors (prosecutor_id);

CREATE TABLE actor_aliases (
    alias_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id    UUID NOT NULL REFERENCES accountability_actors(actor_id) ON DELETE CASCADE,
    alias       TEXT NOT NULL,
    source      TEXT NOT NULL,
    confidence  REAL NOT NULL CHECK (confidence >= 0 AND confidence <= 1),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX actor_aliases_uidx
    ON actor_aliases (actor_id, lower(alias), source);

CREATE TABLE actor_case_links (
    link_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id     UUID NOT NULL REFERENCES accountability_actors(actor_id) ON DELETE CASCADE,
    case_id      UUID NOT NULL REFERENCES court_cases(case_id) ON DELETE CASCADE,
    role_in_case TEXT NOT NULL
                 CHECK (role_in_case IN ('prosecutor','judge','officer','expert','other')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (actor_id, case_id, role_in_case)
);
CREATE INDEX actor_links_case_idx ON actor_case_links (case_id);
CREATE INDEX actor_links_actor_idx ON actor_case_links (actor_id);

CREATE TABLE actor_score_snapshots (
    snapshot_id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id             UUID NOT NULL REFERENCES accountability_actors(actor_id) ON DELETE CASCADE,
    score                REAL NOT NULL,
    substantiated_findings INT NOT NULL DEFAULT 0,
    substantiated_flags  INT NOT NULL DEFAULT 0,
    corroboration_sources INT NOT NULL DEFAULT 0,
    recent_5yr           INT NOT NULL DEFAULT 0,
    formula              TEXT NOT NULL,
    components           JSONB NOT NULL,
    computed_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX actor_score_actor_idx ON actor_score_snapshots (actor_id, computed_at DESC);

CREATE TABLE legal_action_packages (
    package_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id       UUID NOT NULL REFERENCES accountability_actors(actor_id) ON DELETE CASCADE,
    action_kind    TEXT NOT NULL
                   CHECK (action_kind IN (
                       'criminal_referral','civil_1983','bar_complaint','sentencing_memo'
                   )),
    status         TEXT NOT NULL DEFAULT 'draft'
                   CHECK (status IN ('draft','attorney_reviewed','referred','withdrawn')),
    body_markdown  TEXT NOT NULL,
    payload        JSONB NOT NULL,
    document_hash  TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_by    UUID,
    reviewed_at    TIMESTAMPTZ
);
CREATE INDEX legal_pkg_actor_idx ON legal_action_packages (actor_id);
CREATE INDEX legal_pkg_kind_idx ON legal_action_packages (action_kind, status);

-- Attorney-led gate for any named-individual public display.
CREATE TABLE publication_approvals (
    actor_id     UUID PRIMARY KEY REFERENCES accountability_actors(actor_id) ON DELETE CASCADE,
    approved     BOOLEAN NOT NULL DEFAULT false,
    approved_by  UUID,
    approved_at  TIMESTAMPTZ,
    notes        TEXT,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Seed actors from the demo prosecutors and the public-record judge names
-- already on court_cases. No publication approval is granted here.
INSERT INTO accountability_actors (
    actor_id, role, display_name, normalized_name, office, jurisdiction,
    bar_number, prosecutor_id, fingerprint, metadata
) VALUES
(
    'aaaaaaaa-1111-4111-8111-111111111111',
    'prosecutor',
    'Demo Prosecutor',
    'demo prosecutor',
    'Demo County DA',
    'CA',
    'CA-100001',
    '11111111-1111-1111-1111-111111111111',
    'prosecutor|demo prosecutor|ca|ca-100001|',
    '{"source":"seed"}'
),
(
    'aaaaaaaa-4444-4444-8444-444444444444',
    'prosecutor',
    'Other Prosecutor',
    'other prosecutor',
    'Other County DA',
    'CA',
    'CA-200002',
    '44444444-4444-4444-4444-444444444444',
    'prosecutor|other prosecutor|ca|ca-200002|',
    '{"source":"seed"}'
),
(
    'aaaaaaaa-5555-4555-8555-555555555555',
    'judge',
    'Smith J.',
    'smith',
    NULL,
    'CA',
    NULL,
    NULL,
    'judge|smith|ca||',
    '{"source":"seed"}'
),
(
    'aaaaaaaa-6666-4666-8666-666666666666',
    'judge',
    'Lee J.',
    'lee',
    NULL,
    'CA',
    NULL,
    NULL,
    'judge|lee|ca||',
    '{"source":"seed"}'
),
(
    'aaaaaaaa-7777-4777-8777-777777777777',
    'judge',
    'Nguyen J.',
    'nguyen',
    NULL,
    'CA',
    NULL,
    NULL,
    'judge|nguyen|ca||',
    '{"source":"seed"}'
);

INSERT INTO actor_aliases (actor_id, alias, source, confidence) VALUES
('aaaaaaaa-1111-4111-8111-111111111111', 'Demo Prosecutor', 'prosecutors.name', 1.0),
('aaaaaaaa-4444-4444-8444-444444444444', 'Other Prosecutor', 'prosecutors.name', 1.0),
('aaaaaaaa-5555-4555-8555-555555555555', 'Smith J.', 'court_cases.judge', 1.0),
('aaaaaaaa-6666-4666-8666-666666666666', 'Lee J.', 'court_cases.judge', 1.0),
('aaaaaaaa-7777-4777-8777-777777777777', 'Nguyen J.', 'court_cases.judge', 1.0);

INSERT INTO actor_case_links (actor_id, case_id, role_in_case) VALUES
('aaaaaaaa-1111-4111-8111-111111111111', '22222222-2222-2222-2222-222222222222', 'prosecutor'),
('aaaaaaaa-1111-4111-8111-111111111111', '55555555-5555-5555-5555-555555555555', 'prosecutor'),
('aaaaaaaa-1111-4111-8111-111111111111', '66666666-6666-6666-6666-666666666666', 'prosecutor'),
('aaaaaaaa-4444-4444-8444-444444444444', '77777777-7777-7777-7777-777777777777', 'prosecutor'),
('aaaaaaaa-5555-4555-8555-555555555555', '22222222-2222-2222-2222-222222222222', 'judge'),
('aaaaaaaa-5555-4555-8555-555555555555', '55555555-5555-5555-5555-555555555555', 'judge'),
('aaaaaaaa-6666-4666-8666-666666666666', '66666666-6666-6666-6666-666666666666', 'judge'),
('aaaaaaaa-7777-4777-8777-777777777777', '77777777-7777-7777-7777-777777777777', 'judge');
