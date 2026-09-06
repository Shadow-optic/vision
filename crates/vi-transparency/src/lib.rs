//! Transparency Proof Log — certificate-transparency-style tamper evidence
//! for the published dataset.
//!
//! `snapshot()` hashes every published row (wall entries, statute catalog,
//! substantiated findings, referred packages, ledger events) into a Merkle
//! tree over the sorted row hashes. The tree also commits two chain leaves —
//! the previous snapshot root and the current ledger head hash — so one root
//! authenticates the full snapshot history and pins the ledger tip at capture
//! time. Snapshots are audit artifacts: they change nothing that is published
//! and contain no accusatory content beyond what is already public.
#![forbid(unsafe_code)]

pub mod merkle;
mod sources;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

/// Ledger event type anchoring each snapshot root.
pub const EVENT_SNAPSHOT: &str = "TransparencySnapshot";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("ledger: {0}")]
    Ledger(#[from] vi_ledger::Error),
    #[error("reckoning: {0}")]
    Reckoning(#[from] vi_reckoning::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("tree has no leaves")]
    EmptyTree,
    #[error("stored leaf hash malformed (expected 32 bytes)")]
    CorruptLeaf,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Snapshot {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub merkle_root: Vec<u8>,
    pub tree_size: i32,
    pub table_counts: Value,
    pub prev_root: Option<Vec<u8>>,
    pub ledger_seq: Option<i64>,
}

/// Inclusion proof for one published row in one snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    pub snapshot_id: Uuid,
    pub table: String,
    pub row_id: String,
    pub leaf_index: usize,
    pub tree_size: usize,
    pub leaf_hash: [u8; 32],
    pub path: Vec<merkle::ProofNode>,
}

/// Verify a proof against a snapshot root (32-byte Merkle root).
pub fn verify_proof(proof: &Proof, root: &[u8]) -> bool {
    if proof.tree_size == 0 || proof.leaf_index >= proof.tree_size {
        return false;
    }
    let Ok(root) = <[u8; 32]>::try_from(root) else {
        return false;
    };
    merkle::verify(
        &proof.leaf_hash,
        proof.leaf_index,
        proof.tree_size,
        &proof.path,
        &root,
    )
}

/// Capture a new snapshot of the published dataset. Persists the snapshot row
/// and its leaf inventory, then anchors the root in the ledger.
pub async fn snapshot(pool: &PgPool) -> Result<Snapshot, Error> {
    let mut rows = sources::collect_public_rows(pool).await?;
    let prev = latest_snapshot(pool).await?;
    let head: Option<(i64, String)> = sqlx::query_as(
        "SELECT seq, entry_hash FROM ledger_entries ORDER BY seq DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    // Chain leaves: previous snapshot root and current ledger head. These make
    // the Merkle root itself a commitment to history + ledger state.
    rows.push(sources::LeafRow {
        table: sources::TABLE_CHAIN,
        row_id: "prev_root".into(),
        content: prev
            .as_ref()
            .map(|s| json!(hex_encode(&s.merkle_root)))
            .unwrap_or(Value::Null),
    });
    rows.push(sources::LeafRow {
        table: sources::TABLE_CHAIN,
        row_id: "ledger_head".into(),
        content: match &head {
            Some((seq, hash)) => json!({ "seq": seq, "entry_hash": hash }),
            None => Value::Null,
        },
    });

    let mut hashed: Vec<(sources::LeafRow, [u8; 32])> = rows
        .into_iter()
        .map(|r| {
            let h = sources::leaf_content_hash(&r);
            (r, h)
        })
        .collect();
    hashed.sort_by(|a, b| a.1.cmp(&b.1));
    let hashes: Vec<[u8; 32]> = hashed.iter().map(|(_, h)| *h).collect();
    let root = merkle::merkle_root(&hashes).ok_or(Error::EmptyTree)?;

    let mut counts = serde_json::Map::new();
    for (row, _) in &hashed {
        let entry = counts.entry(row.table.to_string()).or_insert(json!(0));
        *entry = json!(entry.as_i64().unwrap_or(0) + 1);
    }
    let table_counts = Value::Object(counts);

    let prev_root: Option<Vec<u8>> = prev.map(|s| s.merkle_root);
    let ledger_seq: Option<i64> = head.map(|(seq, _)| seq);

    let mut tx = pool.begin().await?;
    let snap = sqlx::query_as::<_, Snapshot>(
        "INSERT INTO transparency_snapshots
           (merkle_root, tree_size, table_counts, prev_root, ledger_seq)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, created_at, merkle_root, tree_size, table_counts, prev_root, ledger_seq",
    )
    .bind(root.as_slice())
    .bind(hashes.len() as i32)
    .bind(&table_counts)
    .bind(prev_root.as_deref())
    .bind(ledger_seq)
    .fetch_one(&mut *tx)
    .await?;

    for (i, (row, h)) in hashed.iter().enumerate() {
        sqlx::query(
            "INSERT INTO transparency_leaves (snapshot_id, leaf_index, table_name, row_id, leaf_hash)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(snap.id)
        .bind(i as i32)
        .bind(row.table)
        .bind(&row.row_id)
        .bind(h.as_slice())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    // Anchor the root in the hash-chained ledger. This deliberately moves the
    // ledger head AFTER capture: the snapshot pins the head as of capture
    // time, and the next snapshot will pin this anchoring event in turn.
    Ledger::new(pool.clone())
        .append(
            EVENT_SNAPSHOT,
            &json!({
                "snapshot_id": snap.id,
                "merkle_root": hex_encode(&snap.merkle_root),
                "tree_size": snap.tree_size,
                "table_counts": snap.table_counts,
                "prev_root": snap.prev_root.as_ref().map(|r| hex_encode(r)),
                "ledger_seq": snap.ledger_seq,
            }),
        )
        .await?;

    Ok(snap)
}

/// Most recent snapshot, if any.
pub async fn latest_snapshot(pool: &PgPool) -> Result<Option<Snapshot>, Error> {
    Ok(sqlx::query_as::<_, Snapshot>(
        "SELECT id, created_at, merkle_root, tree_size, table_counts, prev_root, ledger_seq
           FROM transparency_snapshots
          ORDER BY created_at DESC, id DESC
          LIMIT 1",
    )
    .fetch_optional(pool)
    .await?)
}

/// Audit path proving that `(table, row_id)` was part of a snapshot. Uses the
/// most recent snapshot containing the row; `None` if the row was never
/// snapshotted (e.g. unpublished, or no snapshot taken yet).
pub async fn inclusion_proof(
    pool: &PgPool,
    table: &str,
    row_id: &str,
) -> Result<Option<Proof>, Error> {
    let hit: Option<(Uuid, i32, Vec<u8>)> = sqlx::query_as(
        "SELECT l.snapshot_id, l.leaf_index, l.leaf_hash
           FROM transparency_leaves l
           JOIN transparency_snapshots s ON s.id = l.snapshot_id
          WHERE l.table_name = $1 AND l.row_id = $2
          ORDER BY s.created_at DESC, l.snapshot_id
          LIMIT 1",
    )
    .bind(table)
    .bind(row_id)
    .fetch_optional(pool)
    .await?;
    let Some((snapshot_id, leaf_index, leaf_hash)) = hit else {
        return Ok(None);
    };

    let snap = sqlx::query_as::<_, Snapshot>(
        "SELECT id, created_at, merkle_root, tree_size, table_counts, prev_root, ledger_seq
           FROM transparency_snapshots WHERE id = $1",
    )
    .bind(snapshot_id)
    .fetch_one(pool)
    .await?;

    let stored: Vec<(i32, Vec<u8>)> = sqlx::query_as(
        "SELECT leaf_index, leaf_hash FROM transparency_leaves
          WHERE snapshot_id = $1 ORDER BY leaf_index",
    )
    .bind(snapshot_id)
    .fetch_all(pool)
    .await?;
    let mut hashes = Vec::with_capacity(stored.len());
    for (_, h) in stored {
        hashes.push(<[u8; 32]>::try_from(h.as_slice()).map_err(|_| Error::CorruptLeaf)?);
    }

    let leaf_hash: [u8; 32] =
        <[u8; 32]>::try_from(leaf_hash.as_slice()).map_err(|_| Error::CorruptLeaf)?;
    let path = merkle::prove(&hashes, leaf_index as usize).ok_or(Error::CorruptLeaf)?;

    Ok(Some(Proof {
        snapshot_id,
        table: table.to_string(),
        row_id: row_id.to_string(),
        leaf_index: leaf_index as usize,
        tree_size: snap.tree_size as usize,
        leaf_hash,
        path,
    }))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use sources::LeafRow;

    fn row(table: &'static str, id: &str, v: i64) -> LeafRow {
        LeafRow {
            table,
            row_id: id.into(),
            content: json!({ "v": v }),
        }
    }

    #[test]
    fn leaf_hash_is_deterministic_and_content_sensitive() {
        let a = row("findings", "r1", 1);
        let b = row("findings", "r1", 1);
        assert_eq!(sources::leaf_content_hash(&a), sources::leaf_content_hash(&b));
        let tampered = row("findings", "r1", 2);
        assert_ne!(
            sources::leaf_content_hash(&a),
            sources::leaf_content_hash(&tampered)
        );
        // Same content under a different table is a different leaf.
        let other_table = row("referrals", "r1", 1);
        assert_ne!(
            sources::leaf_content_hash(&a),
            sources::leaf_content_hash(&other_table)
        );
    }

    #[test]
    fn end_to_end_pure_tree_round_trip_and_tamper() {
        // Build a tree exactly as snapshot() does, minus the DB.
        let rows = vec![
            row("findings", "f1", 10),
            row("findings", "f2", 20),
            row("wall_entries", "w1", 30),
            row(sources::TABLE_CHAIN, "prev_root", 0),
            row(sources::TABLE_CHAIN, "ledger_head", 0),
        ];
        let mut hashed: Vec<(LeafRow, [u8; 32])> = rows
            .into_iter()
            .map(|r| {
                let h = sources::leaf_content_hash(&r);
                (r, h)
            })
            .collect();
        hashed.sort_by(|a, b| a.1.cmp(&b.1));
        let hashes: Vec<[u8; 32]> = hashed.iter().map(|(_, h)| *h).collect();
        let root = merkle::merkle_root(&hashes).unwrap();

        for (i, (r, h)) in hashed.iter().enumerate() {
            let proof = Proof {
                snapshot_id: Uuid::nil(),
                table: r.table.to_string(),
                row_id: r.row_id.clone(),
                leaf_index: i,
                tree_size: hashes.len(),
                leaf_hash: *h,
                path: merkle::prove(&hashes, i).unwrap(),
            };
            assert!(verify_proof(&proof, &root));
            // Flip one content byte: recomputed leaf no longer verifies.
            let forged = LeafRow {
                content: json!({ "v": 999 }),
                ..proof_row(&proof)
            };
            let mut bad = proof;
            bad.leaf_hash = sources::leaf_content_hash(&forged);
            assert!(!verify_proof(&bad, &root));
        }
    }

    fn proof_row(p: &Proof) -> LeafRow {
        LeafRow {
            table: Box::leak(p.table.clone().into_boxed_str()),
            row_id: p.row_id.clone(),
            content: Value::Null,
        }
    }
}
