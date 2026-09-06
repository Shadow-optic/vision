//! DB-backed integration tests. Skipped silently when DATABASE_URL is unset
//! or unreachable — pure unit tests (merkle.rs, lib.rs) are the CI gate;
//! these run wherever a live Postgres with the workspace migrations exists.

use sqlx::PgPool;

async fn try_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    PgPool::connect(&url).await.ok()
}

#[tokio::test]
async fn snapshot_proof_verify_and_tamper() {
    let Some(pool) = try_pool().await else {
        eprintln!("DATABASE_URL unreachable; skipping DB integration test");
        return;
    };
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();

    let snap = vi_transparency::snapshot(&pool).await.unwrap();
    assert!(snap.merkle_root.len() == 32);
    assert!(snap.tree_size >= 2); // two chain leaves at minimum

    // A second snapshot chains to the first.
    let snap2 = vi_transparency::snapshot(&pool).await.unwrap();
    assert_eq!(snap2.prev_root.as_deref(), Some(snap.merkle_root.as_slice()));
    assert!(snap2.ledger_seq.is_some()); // snapshot events anchor in the ledger

    // The ledger-head chain leaf is always present.
    let proof = vi_transparency::inclusion_proof(&pool, "__chain__", "ledger_head")
        .await
        .unwrap()
        .expect("chain leaf must be in the latest snapshot");
    assert!(vi_transparency::verify_proof(&proof, &snap2.merkle_root));

    // Tamper: flip one byte of the leaf hash → verification fails.
    let mut forged = proof.clone();
    forged.leaf_hash[0] ^= 0x01;
    assert!(!vi_transparency::verify_proof(&forged, &snap2.merkle_root));
    // Wrong root fails too.
    assert!(!vi_transparency::verify_proof(&proof, &snap.merkle_root));

    // A row that was never snapshotted has no proof.
    let none = vi_transparency::inclusion_proof(&pool, "findings", "no-such-row")
        .await
        .unwrap();
    assert!(none.is_none());
}
