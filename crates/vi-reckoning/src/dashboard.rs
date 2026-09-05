//! Public Accountability Register (Wall of Injustice).
//!
//! Counsel review (`review_status = substantiated`) is the publication gate.
//! Once a finding drawn from public records is substantiated, the official's
//! public-record identity and those findings are published. The engine does
//! not charge anyone. Counsel may place a hold for victim privacy or a
//! pending correction — that is a suppression, not a second opt-in.
//!
//! Pending TrustScript flags never appear. No photos, home addresses, or
//! private contact data.
#![forbid(unsafe_code)]

use serde::Serialize;
use serde_json::{json, Value};
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

use crate::Error;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct WallEntry {
    pub actor_id: Uuid,
    pub role: String,
    pub display_name: String,
    pub office: Option<String>,
    pub jurisdiction: String,
    pub bar_number: Option<String>,
    pub badge_number: Option<String>,
    pub substantiated_findings: i64,
    pub public_records: Value,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TrackerRow {
    pub package_id: Uuid,
    pub actor_id: Uuid,
    pub display_name: String,
    pub action_kind: String,
    pub status: String,
}

pub async fn wall(pool: &PgPool) -> Result<Vec<WallEntry>, Error> {
    Ok(sqlx::query_as::<_, WallEntry>(
        "SELECT a.actor_id, a.role, a.display_name, a.office, a.jurisdiction,
                a.bar_number, a.badge_number,
                (SELECT COUNT(*) FROM constitutional_findings f
                  WHERE f.prosecutor_id = a.prosecutor_id
                    AND f.review_status = 'substantiated') AS substantiated_findings,
                COALESCE((
                  SELECT jsonb_agg(jsonb_build_object(
                           'finding_type', f.finding_type,
                           'citation', f.source_citation,
                           'summary', f.summary,
                           'finding_date', f.finding_date,
                           'source_url', f.source_url
                         ) ORDER BY f.finding_date)
                  FROM constitutional_findings f
                  WHERE f.prosecutor_id = a.prosecutor_id
                    AND f.review_status = 'substantiated'
                ), '[]'::jsonb) AS public_records,
                CASE
                  WHEN EXISTS (
                    SELECT 1 FROM legal_action_packages p
                     WHERE p.actor_id = a.actor_id AND p.status = 'referred'
                  ) THEN 'referred'
                  ELSE 'substantiated'
                END AS status
         FROM accountability_actors a
         WHERE EXISTS (
           SELECT 1 FROM constitutional_findings f
            WHERE f.prosecutor_id = a.prosecutor_id
              AND f.review_status = 'substantiated'
         )
         AND NOT EXISTS (
           SELECT 1 FROM publication_approvals pa
            WHERE pa.actor_id = a.actor_id AND pa.approved = false
         )
         ORDER BY substantiated_findings DESC, a.display_name",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn wall_profile(pool: &PgPool, actor_id: Uuid) -> Result<WallEntry, Error> {
    sqlx::query_as::<_, WallEntry>(
        "SELECT a.actor_id, a.role, a.display_name, a.office, a.jurisdiction,
                a.bar_number, a.badge_number,
                (SELECT COUNT(*) FROM constitutional_findings f
                  WHERE f.prosecutor_id = a.prosecutor_id
                    AND f.review_status = 'substantiated') AS substantiated_findings,
                COALESCE((
                  SELECT jsonb_agg(jsonb_build_object(
                           'finding_type', f.finding_type,
                           'citation', f.source_citation,
                           'summary', f.summary,
                           'finding_date', f.finding_date,
                           'source_url', f.source_url
                         ) ORDER BY f.finding_date)
                  FROM constitutional_findings f
                  WHERE f.prosecutor_id = a.prosecutor_id
                    AND f.review_status = 'substantiated'
                ), '[]'::jsonb) AS public_records,
                CASE
                  WHEN EXISTS (
                    SELECT 1 FROM legal_action_packages p
                     WHERE p.actor_id = a.actor_id AND p.status = 'referred'
                  ) THEN 'referred'
                  ELSE 'substantiated'
                END AS status
         FROM accountability_actors a
         WHERE a.actor_id = $1
           AND EXISTS (
             SELECT 1 FROM constitutional_findings f
              WHERE f.prosecutor_id = a.prosecutor_id
                AND f.review_status = 'substantiated'
           )
           AND NOT EXISTS (
             SELECT 1 FROM publication_approvals pa
              WHERE pa.actor_id = a.actor_id AND pa.approved = false
           )",
    )
    .bind(actor_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound)
}

pub async fn tracker(pool: &PgPool) -> Result<Vec<TrackerRow>, Error> {
    Ok(sqlx::query_as::<_, TrackerRow>(
        r#"SELECT p.package_id, p.actor_id, a.display_name, p.action_kind, p.status
           FROM legal_action_packages p
           JOIN accountability_actors a ON a.actor_id = p.actor_id
           WHERE p.status IN ('attorney_reviewed','referred')
             AND EXISTS (
               SELECT 1 FROM constitutional_findings f
                WHERE f.prosecutor_id = a.prosecutor_id
                  AND f.review_status = 'substantiated'
             )
             AND NOT EXISTS (
               SELECT 1 FROM publication_approvals pa
                WHERE pa.actor_id = a.actor_id AND pa.approved = false
             )
           ORDER BY p.created_at DESC
           LIMIT 200"#,
    )
    .fetch_all(pool)
    .await?)
}

/// `approved = true` publishes (default once counsel substantiates).
/// `approved = false` holds the public card (victim privacy / correction).
pub async fn set_publication(
    pool: &PgPool,
    ledger: &Ledger,
    actor_id: Uuid,
    approved: bool,
    approved_by: Option<Uuid>,
    notes: Option<String>,
) -> Result<(), Error> {
    let _ = crate::entity::get(pool, actor_id).await?;
    if approved {
        let findings: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM constitutional_findings f
             JOIN accountability_actors a ON a.prosecutor_id = f.prosecutor_id
             WHERE a.actor_id = $1 AND f.review_status = 'substantiated'",
        )
        .bind(actor_id)
        .fetch_one(pool)
        .await?;
        if findings == 0 {
            return Err(Error::PublicationBlocked);
        }
    }

    sqlx::query(
        "INSERT INTO publication_approvals (actor_id, approved, approved_by, approved_at, notes, updated_at)
         VALUES ($1,$2,$3, CASE WHEN $2 THEN now() ELSE NULL END, $4, now())
         ON CONFLICT (actor_id) DO UPDATE
           SET approved = EXCLUDED.approved,
               approved_by = EXCLUDED.approved_by,
               approved_at = CASE WHEN EXCLUDED.approved THEN now() ELSE NULL END,
               notes = EXCLUDED.notes,
               updated_at = now()",
    )
    .bind(actor_id)
    .bind(approved)
    .bind(approved_by)
    .bind(&notes)
    .execute(pool)
    .await?;

    ledger
        .append(
            vi_ledger::events::PUBLICATION_REVIEWED,
            &json!({
                "actor_id": actor_id,
                "approved": approved,
                "effect": if approved { "publish" } else { "hold" },
            }),
        )
        .await?;
    Ok(())
}
