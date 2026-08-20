-- Demo data for local / CI e2e. Production deployments may skip this file
-- (sqlx applies every migration once; treat this as the MVP fixture set).

INSERT INTO prosecutors (prosecutor_id, name, office, jurisdiction) VALUES
('11111111-1111-1111-1111-111111111111', 'Demo Prosecutor', 'Demo County DA', 'CA'),
('44444444-4444-4444-4444-444444444444', 'Other Prosecutor', 'Other County DA', 'CA');

INSERT INTO court_cases (
    case_id, docket_number, jurisdiction, court_level, charge_category,
    prosecutor_id, judge, defendant_race, evidence_strength,
    outcome, plea_offered, plea_accepted, plea_offer_months, sentence_months,
    filing_date, disposition_date,
    court_location_lat, court_location_lng
) VALUES
(
    '22222222-2222-2222-2222-222222222222', 'DEMO-2024-001', 'CA', 'superior', 'drug',
    '11111111-1111-1111-1111-111111111111', 'Smith J.', 'Black', 'weak',
    'conviction', true, false, 12, 36,
    '2024-01-15', '2024-11-02',
    37.7749, -122.4194
),
(
    '55555555-5555-5555-5555-555555555555', 'DEMO-2024-002', 'CA', 'superior', 'drug',
    '11111111-1111-1111-1111-111111111111', 'Smith J.', 'White', 'mixed',
    'conviction', true, false, 12, 18,
    '2024-02-01', '2024-10-15',
    37.7749, -122.4194
),
(
    '66666666-6666-6666-6666-666666666666', 'DEMO-2024-003', 'CA', 'superior', 'assault',
    '11111111-1111-1111-1111-111111111111', 'Lee J.', 'Black', 'strong',
    'conviction', true, true, 24, 24,
    '2024-03-01', '2024-09-01',
    37.7749, -122.4194
),
(
    '77777777-7777-7777-7777-777777777777', 'OTHER-2024-001', 'CA', 'superior', 'drug',
    '44444444-4444-4444-4444-444444444444', 'Nguyen J.', 'Hispanic', 'weak',
    'conviction', true, false, 6, 8,
    '2024-04-01', '2024-12-01',
    34.0522, -118.2437
);

INSERT INTO court_opinions (opinion_id, case_id, court_level, judge, citation, date_issued, full_text) VALUES
('33333333-3333-3333-3333-333333333333',
 '22222222-2222-2222-2222-222222222222', 'superior', 'Smith J.', 'Demo v. Demo (2024)', '2024-11-02',
 'The defendant moved to suppress evidence obtained during the traffic stop.
  The motion was denied. Body-worn camera footage was referenced by the arresting
  officer but the chain of custody documentation was not produced. A laboratory
  report and forensic worksheet were cited in testimony. The defendant rejected
  a plea offer of twelve months and was convicted at trial and sentenced to
  thirty-six months. Counsel noted that a 911 call recording was never disclosed.');

INSERT INTO abuse_rules (name, source) VALUES
('plea-coercion',
 'when case.plea_sentence_ratio < 0.5 and case.evidence_strength == "weak" then flag "Plea coercion suspected" severity high'),
('selective-prosecution-review',
 'when case.defendant_race == "Black" and case.charge_category == "drug" and case.outcome == "conviction" then flag "Selective prosecution pattern — human review required" severity medium');

INSERT INTO constitutional_findings
    (finding_id, case_id, prosecutor_id, office, jurisdiction, finding_type, court_level,
     judge, finding_date, source_citation, summary, review_status)
VALUES (
    '88888888-8888-8888-8888-888888888888',
    '22222222-2222-2222-2222-222222222222',
    '11111111-1111-1111-1111-111111111111',
    'Demo County DA', 'CA', 'brady', 'superior', 'Smith J.', '2024-11-02',
    'Demo v. Demo (2024)',
    'Opinion notes a suppression motion and plea-trial sentence disparity; full chain-of-custody documentation was not referenced in the disclosed record set.',
    'substantiated'
);
