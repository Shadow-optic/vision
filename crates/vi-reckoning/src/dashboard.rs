//! Public Accountability Register (Wall of Injustice).
//! Named individuals appear only after substantiated findings *and*
//! attorney publication approval. No photos, no private contact data.
#![forbid(unsafe_code)]

use serde::Serialize;
use serde_json::json;
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
    pub substantiated_findings: i64,
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
        r#"SELECT a.actor_id, a.role, a.display_name, a.office, a.jurisdiction,
                  (SELECT COUNT(*) FROM constitutional_findings f
                    WHERE f.prosecutor_id = a.prosecutor_id
                      AND f.review_status = 'substantiated') AS substantiated_findings,
                  CASE
                    WHEN EXISTS (
                      SELECT 1 FROM legal_action_packages p
                       WHERE p.actor_id = a.actor_id AND p.status = 'referred'
                    ) THEN 'referred'
                    ELSE 'substantiated'
                  END AS status
           FROM accountability_actors a
           JOIN publication_approvals pa ON pa.actor_id = a.actor_id AND pa.approved = true
           WHERE EXISTS (
             SELECT 1 FROM constitutional_findings f
              WHERE f.prosecutor_id = a.prosecutor_id
                AND f.review_status = 'substantiated'
           )
           ORDER BY substantiated_findings DESC, a.display_name"#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn tracker(pool: &PgPool) -> Result<Vec<TrackerRow>, Error> {
    Ok(sqlx::query_as::<_, TrackerRow>(
        r#"SELECT p.package_id, p.actor_id, a.display_name, p.action_kind, p.status
           FROM legal_action_packages p
           JOIN accountability_actors a ON a.actor_id = p.actor_id
           JOIN publication_approvals pa ON pa.actor_id = a.actor_id AND pa.approved = true
           WHERE p.status IN ('attorney_reviewed','referred')
           ORDER BY p.created_at DESC
           LIMIT 200"#,
    )
    .fetch_all(pool)
    .await?)
}

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
            }),
        )
        .await?;
    Ok(())
}
