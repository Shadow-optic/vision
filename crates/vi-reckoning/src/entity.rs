//! Entity resolution for public officials.
//! Deterministic fingerprints first; probabilistic name match second.
//! Identifiers are public-record fields only (name, bar, badge, office).
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use vi_ledger::Ledger;

use crate::Error;

const SUFFIXES: &[&str] = &[
    "jr",
    "sr",
    "esq",
    "esquire",
    "hon",
    "honorable",
    "judge",
    "justice",
    "j",
    "iii",
    "ii",
    "iv",
];

const ROLES: &[&str] = &["prosecutor", "officer", "judge", "expert", "other"];

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Actor {
    pub actor_id: Uuid,
    pub role: String,
    pub display_name: String,
    pub normalized_name: String,
    pub office: Option<String>,
    pub jurisdiction: String,
    pub bar_number: Option<String>,
    pub badge_number: Option<String>,
    pub prosecutor_id: Option<Uuid>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResolveQuery {
    pub role: String,
    pub name: String,
    pub jurisdiction: String,
    pub office: Option<String>,
    pub bar_number: Option<String>,
    pub badge_number: Option<String>,
    pub prosecutor_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolveHit {
    pub actor: Actor,
    pub method: &'static str,
    pub confidence: f64,
}

pub fn validate_role(role: &str) -> Result<(), Error> {
    if ROLES.contains(&role) {
        Ok(())
    } else {
        Err(Error::InvalidRole(role.to_string()))
    }
}

pub fn normalize_name(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    let cleaned: String = lowered
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned
        .split_whitespace()
        .filter(|tok| !SUFFIXES.contains(tok))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn fingerprint(
    role: &str,
    normalized_name: &str,
    jurisdiction: &str,
    bar_number: Option<&str>,
    badge_number: Option<&str>,
) -> String {
    let jur = jurisdiction.trim().to_lowercase();
    let bar = bar_number.unwrap_or("").trim().to_lowercase();
    let badge = badge_number.unwrap_or("").trim().to_lowercase();
    format!("{role}|{normalized_name}|{jur}|{bar}|{badge}")
}

/// Iterative Levenshtein distance. Fine for short official names.
pub fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b_chars.len()).collect();
    let mut curr = vec![0; b_chars.len() + 1];
    for (i, ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_chars.len()]
}

pub fn name_similarity(a: &str, b: &str) -> f64 {
    let na = normalize_name(a);
    let nb = normalize_name(b);
    if na.is_empty() && nb.is_empty() {
        return 1.0;
    }
    let max = na.chars().count().max(nb.chars().count()) as f64;
    if max == 0.0 {
        return 1.0;
    }
    1.0 - (levenshtein(&na, &nb) as f64 / max)
}

pub async fn get(pool: &PgPool, actor_id: Uuid) -> Result<Actor, Error> {
    sqlx::query_as::<_, Actor>(
        "SELECT actor_id, role, display_name, normalized_name, office, jurisdiction,
                bar_number, badge_number, prosecutor_id, fingerprint
         FROM accountability_actors WHERE actor_id = $1",
    )
    .bind(actor_id)
    .fetch_optional(pool)
    .await?
    .ok_or(Error::NotFound)
}

pub async fn list(
    pool: &PgPool,
    role: Option<&str>,
    jurisdiction: Option<&str>,
) -> Result<Vec<Actor>, Error> {
    Ok(sqlx::query_as::<_, Actor>(
        "SELECT actor_id, role, display_name, normalized_name, office, jurisdiction,
                bar_number, badge_number, prosecutor_id, fingerprint
         FROM accountability_actors
         WHERE ($1::text IS NULL OR role = $1)
           AND ($2::text IS NULL OR jurisdiction = $2)
         ORDER BY display_name",
    )
    .bind(role)
    .bind(jurisdiction)
    .fetch_all(pool)
    .await?)
}

/// Resolve or create an actor. Deterministic keys (bar/badge/fingerprint)
/// win; otherwise a high-confidence name match in the same jurisdiction.
pub async fn resolve(
    pool: &PgPool,
    ledger: &Ledger,
    q: &ResolveQuery,
) -> Result<ResolveHit, Error> {
    validate_role(&q.role)?;
    let norm = normalize_name(&q.name);
    if norm.is_empty() {
        return Err(Error::InvalidName);
    }
    let fp = fingerprint(
        &q.role,
        &norm,
        &q.jurisdiction,
        q.bar_number.as_deref(),
        q.badge_number.as_deref(),
    );

    if let Some(bar) = q.bar_number.as_deref().filter(|s| !s.is_empty()) {
        if let Some(actor) = sqlx::query_as::<_, Actor>(
            "SELECT actor_id, role, display_name, normalized_name, office, jurisdiction,
                    bar_number, badge_number, prosecutor_id, fingerprint
             FROM accountability_actors
             WHERE lower(bar_number) = lower($1) AND role = $2",
        )
        .bind(bar)
        .bind(&q.role)
        .fetch_optional(pool)
        .await?
        {
            return Ok(ResolveHit {
                actor,
                method: "bar_number",
                confidence: 1.0,
            });
        }
    }

    if let Some(badge) = q.badge_number.as_deref().filter(|s| !s.is_empty()) {
        if let Some(actor) = sqlx::query_as::<_, Actor>(
            "SELECT actor_id, role, display_name, normalized_name, office, jurisdiction,
                    bar_number, badge_number, prosecutor_id, fingerprint
             FROM accountability_actors
             WHERE lower(badge_number) = lower($1) AND role = $2 AND jurisdiction = $3",
        )
        .bind(badge)
        .bind(&q.role)
        .bind(&q.jurisdiction)
        .fetch_optional(pool)
        .await?
        {
            return Ok(ResolveHit {
                actor,
                method: "badge_number",
                confidence: 1.0,
            });
        }
    }

    if let Some(actor) = sqlx::query_as::<_, Actor>(
        "SELECT actor_id, role, display_name, normalized_name, office, jurisdiction,
                bar_number, badge_number, prosecutor_id, fingerprint
         FROM accountability_actors WHERE fingerprint = $1",
    )
    .bind(&fp)
    .fetch_optional(pool)
    .await?
    {
        return Ok(ResolveHit {
            actor,
            method: "fingerprint",
            confidence: 1.0,
        });
    }

    let candidates: Vec<Actor> = sqlx::query_as(
        "SELECT actor_id, role, display_name, normalized_name, office, jurisdiction,
                bar_number, badge_number, prosecutor_id, fingerprint
         FROM accountability_actors
         WHERE role = $1 AND jurisdiction = $2",
    )
    .bind(&q.role)
    .bind(&q.jurisdiction)
    .fetch_all(pool)
    .await?;

    let mut best: Option<(Actor, f64)> = None;
    for cand in candidates {
        let sim = name_similarity(&norm, &cand.normalized_name);
        if sim >= 0.86 && best.as_ref().map(|(_, s)| sim > *s).unwrap_or(true) {
            best = Some((cand, sim));
        }
    }
    if let Some((actor, confidence)) = best {
        return Ok(ResolveHit {
            actor,
            method: "name_similarity",
            confidence,
        });
    }

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accountability_actors
         (actor_id, role, display_name, normalized_name, office, jurisdiction,
          bar_number, badge_number, prosecutor_id, fingerprint, metadata)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'{\"source\":\"resolve\"}')",
    )
    .bind(id)
    .bind(&q.role)
    .bind(&q.name)
    .bind(&norm)
    .bind(&q.office)
    .bind(&q.jurisdiction)
    .bind(&q.bar_number)
    .bind(&q.badge_number)
    .bind(q.prosecutor_id)
    .bind(&fp)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO actor_aliases (actor_id, alias, source, confidence)
         VALUES ($1,$2,'resolve',1.0)
         ON CONFLICT DO NOTHING",
    )
    .bind(id)
    .bind(&q.name)
    .execute(pool)
    .await?;

    ledger
        .append(
            vi_ledger::events::ACTOR_RESOLVED,
            &json!({
                "actor_id": id,
                "role": q.role,
                "fingerprint": fp,
                "method": "created",
            }),
        )
        .await?;

    Ok(ResolveHit {
        actor: get(pool, id).await?,
        method: "created",
        confidence: 1.0,
    })
}

/// Upsert actors from the prosecutors table and distinct public-record judges.
pub async fn sync_from_public_records(pool: &PgPool, ledger: &Ledger) -> Result<u64, Error> {
    let prosecutors: Vec<(Uuid, String, String, String)> =
        sqlx::query_as("SELECT prosecutor_id, name, office, jurisdiction FROM prosecutors")
            .fetch_all(pool)
            .await?;

    let mut created = 0u64;
    for (prosecutor_id, name, office, jurisdiction) in prosecutors {
        let hit = resolve(
            pool,
            ledger,
            &ResolveQuery {
                role: "prosecutor".into(),
                name,
                jurisdiction,
                office: Some(office),
                bar_number: None,
                badge_number: None,
                prosecutor_id: Some(prosecutor_id),
            },
        )
        .await?;
        if hit.method == "created" {
            created += 1;
        }
        sqlx::query(
            "UPDATE accountability_actors SET prosecutor_id = $1
             WHERE actor_id = $2 AND prosecutor_id IS NULL",
        )
        .bind(prosecutor_id)
        .bind(hit.actor.actor_id)
        .execute(pool)
        .await?;
    }

    let judges: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT judge, jurisdiction FROM court_cases
         WHERE judge IS NOT NULL AND judge <> ''",
    )
    .fetch_all(pool)
    .await?;
    for (name, jurisdiction) in judges {
        let hit = resolve(
            pool,
            ledger,
            &ResolveQuery {
                role: "judge".into(),
                name,
                jurisdiction,
                office: None,
                bar_number: None,
                badge_number: None,
                prosecutor_id: None,
            },
        )
        .await?;
        if hit.method == "created" {
            created += 1;
        }
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_judicial_suffix() {
        assert_eq!(normalize_name("Smith J."), "smith");
        assert_eq!(normalize_name("Hon. Jane Doe, Esq."), "jane doe");
        assert_eq!(normalize_name("Demo Prosecutor"), "demo prosecutor");
    }

    #[test]
    fn fingerprint_is_stable() {
        let a = fingerprint(
            "prosecutor",
            "demo prosecutor",
            "CA",
            Some("CA-100001"),
            None,
        );
        let b = fingerprint(
            "prosecutor",
            "demo prosecutor",
            "ca",
            Some("ca-100001"),
            None,
        );
        assert_eq!(a, b);
        assert_eq!(a, "prosecutor|demo prosecutor|ca|ca-100001|");
    }

    #[test]
    fn levenshtein_identity() {
        assert_eq!(levenshtein("smith", "smith"), 0);
        assert_eq!(levenshtein("smith", "smyth"), 1);
    }

    #[test]
    fn similar_names_pass_threshold() {
        assert!(name_similarity("Smith J.", "Smith") >= 0.86);
        assert!(name_similarity("Demo Prosecutor", "Other Prosecutor") < 0.86);
    }

    #[test]
    fn role_validation() {
        assert!(validate_role("prosecutor").is_ok());
        assert!(validate_role("vigilante").is_err());
    }
}
