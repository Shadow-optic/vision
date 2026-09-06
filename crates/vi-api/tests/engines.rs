//! Integration tests. Ignored unless DATABASE_URL is set (CI sets it).
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

const DEMO_CASE: &str = "22222222-2222-2222-2222-222222222222";

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .ok()
}

#[tokio::test]
async fn migrate_seed_and_engines() {
    let Some(pool) = pool().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    vi_db::migrate(&pool).await.expect("migrate");
    vi_db::ping(&pool).await.expect("ping");

    let ledger = vi_ledger::Ledger::new(pool.clone());
    let case_id = Uuid::parse_str(DEMO_CASE).unwrap();

    let ctx = vi_db::case_context(&pool, case_id)
        .await
        .unwrap()
        .expect("seeded case");
    let ratio = ctx
        .pointer("/case/plea_sentence_ratio")
        .and_then(|v| v.as_f64())
        .expect("derived ratio");
    assert!((ratio - 12.0 / 36.0).abs() < 1e-9);

    let entry = ledger
        .append(
            vi_ledger::events::FLAG_REVIEWED,
            &json!({"test": true, "case_id": case_id}),
        )
        .await
        .unwrap();
    assert!(entry.seq >= 1);
    let report = ledger.verify().await.unwrap();
    assert!(report.ok, "ledger chain must verify");

    let items = vi_brady_recon::extractor::derive_expected_for_case(&pool, case_id)
        .await
        .unwrap();
    assert!(
        items.len() >= 4,
        "demo opinion should yield bodycam/coc/lab/911/worksheet, got {items:?}"
    );
    let recon = vi_brady_recon::reconcile::reconcile(&pool, &ledger, case_id)
        .await
        .unwrap();
    assert!(recon.gap_count >= 1);

    let fp = vi_monell_atlas::stats::office_fingerprint(&pool, "Demo County DA", Some("CA"))
        .await
        .unwrap();
    assert_eq!(fp.office, "Demo County DA");
    assert!(fp.total_substantiated >= 1);
    let md = vi_monell_atlas::report::render(&fp).unwrap();
    assert!(md.contains("Attorney Work Product"));

    let (dist, _) =
        vi_trial_penalty::distribution::by_office(&pool, &ledger, "Demo County DA", Some("CA"))
            .await
            .unwrap();
    assert!(dist.n >= 2, "two plea-rejected convictions in seed");
    let mean = dist.mean_ratio.unwrap();
    // 36/12=3 and 18/12=1.5 → mean 2.25
    assert!((mean - 2.25).abs() < 0.01);

    let disp =
        vi_trial_penalty::disparity::racial_disparity(&pool, Some("drug"), "Black", "White", 2.0)
            .await
            .unwrap();
    assert_eq!(disp.n_a, 1);
    assert_eq!(disp.n_b, 1);
}

#[tokio::test]
async fn trustscript_seed_rules_fire() {
    let Some(pool) = pool().await else {
        return;
    };
    vi_db::migrate(&pool).await.unwrap();
    let case_id = Uuid::parse_str(DEMO_CASE).unwrap();
    let ctx = vi_db::case_context(&pool, case_id).await.unwrap().unwrap();
    let src = sqlx::query_scalar::<_, String>(
        "SELECT source FROM abuse_rules WHERE name='plea-coercion'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let rule = vi_trustscript::parse_rule(&src).unwrap();
    let flag = vi_trustscript::evaluate(&rule, &ctx).expect("plea-coercion should fire on seed");
    assert_eq!(flag.severity, vi_trustscript::Severity::High);

    assert_eq!(ctx["constitution"]["circuit"], "CA9");
    assert_eq!(
        ctx["constitution"]["amend_06"]["trial_right_pressure"],
        true
    );
    let sixth = sqlx::query_scalar::<_, String>(
        "SELECT source FROM abuse_rules WHERE name='sixth-amendment-trial-pressure'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let sixth_rule = vi_trustscript::parse_rule(&sixth).unwrap();
    let sixth_flag = vi_trustscript::evaluate(&sixth_rule, &ctx).expect("sixth amendment rule");
    assert_eq!(sixth_flag.severity, vi_trustscript::Severity::High);

    let ledger = vi_ledger::Ledger::new(pool.clone());
    let (id, report, md) = vi_constitution::db::screen_case(&pool, &ledger, case_id)
        .await
        .unwrap();
    assert!(!id.is_nil());
    assert!(report.hits.len() >= 2);
    assert!(md.contains("not legal advice"));
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM jurisdiction_circuits WHERE kind='state'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(n, 50);
}

#[tokio::test]
async fn reckoning_scores_only_substantiated_evidence() {
    let Some(pool) = pool().await else {
        return;
    };
    vi_db::migrate(&pool).await.unwrap();
    let ledger = vi_ledger::Ledger::new(pool.clone());
    let actor_id = Uuid::parse_str("aaaaaaaa-1111-4111-8111-111111111111").unwrap();

    let score = vi_reckoning::score_actor(&pool, Some(&ledger), actor_id)
        .await
        .unwrap();
    assert!(
        (score.score - 28.0).abs() < 1e-9,
        "one substantiated Brady finding, one source, recent → 28, got {}",
        score.score
    );

    let (pkg, md) = vi_reckoning::generate(&pool, &ledger, actor_id, "bar_complaint")
        .await
        .unwrap();
    assert_eq!(pkg.action_kind, "bar_complaint");
    assert!(md.contains("State Bar of California"));
    assert!(md.contains("does not seek a predetermined sanction"));

    let (_, sent) = vi_reckoning::generate(&pool, &ledger, actor_id, "sentencing_memo")
        .await
        .unwrap();
    assert!(sent.contains("Sentencing Advocacy"));
    assert!(sent.contains("statutory maximum"));
    assert!(sent.contains("Life imprisonment is not unlocked on this record"));

    let judge = Uuid::parse_str("aaaaaaaa-5555-4555-8555-555555555555").unwrap();
    let err = vi_reckoning::generate(&pool, &ledger, judge, "criminal_referral")
        .await
        .expect_err("judge has no substantiated findings");
    assert!(matches!(err, vi_reckoning::Error::InsufficientEvidence));

    let wall = vi_reckoning::wall(&pool).await.unwrap();
    assert!(
        wall.iter()
            .any(|e| e.actor_id == actor_id && e.substantiated_findings >= 1),
        "substantiated public-record findings publish after counsel review"
    );
    assert!(!wall.iter().any(|e| e.actor_id == judge));
}
