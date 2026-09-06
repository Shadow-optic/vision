//! Row collection for the published dataset. "Published" is defined by the
//! same gates the public API uses: the wall gate from vi-reckoning, the static
//! statute catalog, substantiated findings, referred legal-action packages,
//! and the full ledger. Pending/rejected rows never enter the tree.
#![forbid(unsafe_code)]

use serde_json::Value;
use sqlx::PgPool;

use crate::merkle;
use crate::Error;

pub const TABLE_WALL: &str = "wall_entries";
pub const TABLE_STATUTES: &str = "statutes";
pub const TABLE_FINDINGS: &str = "findings";
pub const TABLE_REFERRALS: &str = "referrals";
pub const TABLE_LEDGER: &str = "ledger_events";
/// Pseudo-table for the two chain leaves (previous root, ledger head).
/// Sorts ahead of every real table name and can never collide with one.
pub const TABLE_CHAIN: &str = "__chain__";

#[derive(Debug, Clone)]
pub struct LeafRow {
    pub table: &'static str,
    pub row_id: String,
    pub content: Value,
}

/// Canonical leaf content: table || NUL || row_id || NUL || serde_json bytes.
/// serde_json maps are key-ordered (BTreeMap), so encoding is deterministic.
pub fn leaf_content_hash(row: &LeafRow) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(row.table.as_bytes());
    buf.push(0x00);
    buf.extend_from_slice(row.row_id.as_bytes());
    buf.push(0x00);
    buf.extend_from_slice(&serde_json::to_vec(&row.content).expect("Value always serializes"));
    merkle::hash_leaf(&buf)
}

/// All published rows, unsorted. Empty corpus → empty vec (never fabricated).
pub async fn collect_public_rows(pool: &PgPool) -> Result<Vec<LeafRow>, Error> {
    let mut rows = Vec::new();

    // Wall entries: exactly the actors the public register would show, via
    // vi-reckoning's own publication gate (substantiated finding, no hold).
    for entry in vi_reckoning::wall(pool).await? {
        rows.push(LeafRow {
            table: TABLE_WALL,
            row_id: entry.actor_id.to_string(),
            content: serde_json::to_value(&entry)?,
        });
    }

    // Statute catalog (static, attorney-research content served publicly).
    for s in vi_reckoning::statutes::STATUTES {
        rows.push(LeafRow {
            table: TABLE_STATUTES,
            row_id: s.citation.to_string(),
            content: serde_json::to_value(s)?,
        });
    }

    // Substantiated constitutional findings.
    let findings: Vec<(String, Value)> = sqlx::query_as(
        "SELECT f.finding_id::text, to_jsonb(f)
           FROM constitutional_findings f
          WHERE f.review_status = 'substantiated'
          ORDER BY f.finding_id",
    )
    .fetch_all(pool)
    .await?;
    for (row_id, doc) in findings {
        rows.push(LeafRow {
            table: TABLE_FINDINGS,
            row_id,
            content: doc,
        });
    }

    // Referred legal-action packages.
    let referrals: Vec<(String, Value)> = sqlx::query_as(
        "SELECT p.package_id::text, to_jsonb(p)
           FROM legal_action_packages p
          WHERE p.status = 'referred'
          ORDER BY p.package_id",
    )
    .fetch_all(pool)
    .await?;
    for (row_id, doc) in referrals {
        rows.push(LeafRow {
            table: TABLE_REFERRALS,
            row_id,
            content: doc,
        });
    }

    // Every ledger event (the ledger is the public audit spine).
    let events: Vec<(String, Value)> = sqlx::query_as(
        "SELECT l.seq::text, to_jsonb(l) FROM ledger_entries l ORDER BY l.seq",
    )
    .fetch_all(pool)
    .await?;
    for (row_id, doc) in events {
        rows.push(LeafRow {
            table: TABLE_LEDGER,
            row_id,
            content: doc,
        });
    }

    Ok(rows)
}
