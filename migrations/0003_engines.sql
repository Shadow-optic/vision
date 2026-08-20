-- ===== Monell Atlas =====
CREATE TABLE constitutional_findings (
    finding_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID REFERENCES court_cases(case_id) ON DELETE SET NULL,
    prosecutor_id UUID REFERENCES prosecutors(prosecutor_id) ON DELETE SET NULL,
    office TEXT NOT NULL,
    jurisdiction TEXT NOT NULL,
    finding_type TEXT NOT NULL
        CHECK (finding_type IN (
            'brady','giglio','batson','discovery','witness_subornation',
            'sanction','due_process','other'
        )),
    court_level TEXT,
    judge TEXT,
    finding_date DATE,
    source_citation TEXT,
    source_url TEXT,
    summary TEXT NOT NULL,
    document_hash TEXT,
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (review_status IN ('pending','substantiated','rejected')),
    reviewed_by UUID,
    reviewed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX cf_office_idx         ON constitutional_findings(office);
CREATE INDEX cf_jurisdiction_idx   ON constitutional_findings(jurisdiction);
CREATE INDEX cf_type_idx           ON constitutional_findings(finding_type);
CREATE INDEX cf_review_status_idx  ON constitutional_findings(review_status);
CREATE INDEX cf_finding_date_idx   ON constitutional_findings(finding_date);

-- ===== Brady Reconciliation =====
CREATE TABLE expected_evidence_items (
    item_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID REFERENCES court_cases(case_id) ON DELETE CASCADE,
    item_type TEXT NOT NULL
        CHECK (item_type IN (
            'witness_interview','bodycam','lab_report','chain_of_custody',
            'informant_benefit','911_call','forensic_worksheet','other'
        )),
    description TEXT NOT NULL,
    description_norm TEXT GENERATED ALWAYS AS (lower(description)) STORED,
    source_reference TEXT NOT NULL,
    first_seen_date TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (case_id, item_type, description_norm, source_reference)
);
CREATE INDEX eei_case_idx ON expected_evidence_items(case_id);

CREATE TABLE disclosed_evidence_items (
    item_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID REFERENCES court_cases(case_id) ON DELETE CASCADE,
    item_type TEXT NOT NULL,
    description TEXT NOT NULL,
    disclosed_date DATE,
    disclosed_by UUID REFERENCES prosecutors(prosecutor_id) ON DELETE SET NULL,
    source_url TEXT,
    raw_metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX dei_case_idx ON disclosed_evidence_items(case_id);

CREATE TABLE brady_recon_runs (
    run_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID REFERENCES court_cases(case_id) ON DELETE CASCADE,
    run_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    gaps_found INT NOT NULL DEFAULT 0,
    report JSONB NOT NULL,
    review_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (review_status IN ('pending','substantiated','rejected'))
);
CREATE INDEX brc_case_idx ON brady_recon_runs(case_id);
