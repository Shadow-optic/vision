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

/// Titles that sit in front of a name. Stripped only from the front, because a
/// word like "Justice" or "Chief" further in is more likely part of the name.
const TITLE_PREFIXES: &[&str] = &[
    "chief",
    "associate",
    "senior",
    "presiding",
    "acting",
    "magistrate",
    "district",
    "circuit",
    "administrative",
    "the",
    "mr",
    "mrs",
    "ms",
];

/// Strings a court record puts in a judge field when it is naming the court
/// rather than a person. None of these is an individual.
const COLLECTIVE_MARKERS: &[&str] = &[
    "per curiam",
    "percuriam",
    "by the court",
    "the court",
    "en banc",
    "panel",
    "unassigned",
    "unknown",
    "none",
    "n a",
    "not available",
    "unpublished",
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
    let mut tokens: Vec<&str> = cleaned
        .split_whitespace()
        .filter(|tok| !SUFFIXES.contains(tok))
        .collect();
    // "Associate Judge Easterly" and "Easterly" are one person. Leaving the
    // title in would file them as two, and a record split across two identities
    // is a record nobody is accountable for.
    while tokens.first().is_some_and(|t| TITLE_PREFIXES.contains(t)) {
        tokens.remove(0);
    }
    tokens.join(" ")
}

/// What a court record's judge or counsel field actually names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Officials {
    /// One entry per individual named, cleaned of titles.
    Individuals(Vec<String>),
    /// The field names the court acting as a body, not a person.
    Collective { reason: String },
    /// The field names more than one individual, but where one name ends and
    /// the next begins cannot be read from the text alone.
    Ambiguous { reason: String },
}

/// Read a court record's judge field as the individuals it names.
///
/// This is the hinge of individual accountability. A record that says
/// "Mathias, DeBoer, Kenworthy" names three judges; stored whole it becomes a
/// fourth person who does not exist, and the three real ones accumulate
/// nothing. So panels are split.
///
/// Where the split is genuinely unreadable — "Smith, John" is either one judge
/// written surname-first or two judges — nothing is attributed. Guessing would
/// put one official's conduct on another's record, which is the same wrong this
/// system exists to answer. An unreadable field is a question for a human, and
/// [`Officials::Ambiguous`] is how it asks.
pub fn parse_officials(raw: &str) -> Officials {
    let cleaned = strip_extraction_artifacts(raw);
    if cleaned.is_empty() {
        return Officials::Collective {
            reason: "the field is empty".into(),
        };
    }
    if let Some(marker) = collective_marker(&cleaned) {
        return Officials::Collective {
            reason: format!("‘{marker}’ names the court, not an individual"),
        };
    }

    let mut names = Vec::new();
    for segment in split_on_separators(&cleaned) {
        match read_segment(&segment) {
            Some(found) => names.extend(found),
            None => {
                return Officials::Ambiguous {
                    reason: format!(
                        "‘{segment}’ is either one name written surname-first or several \
                         names; the text does not say which"
                    ),
                }
            }
        }
    }

    names.retain(|n| !normalize_name(n).is_empty());
    if names.is_empty() {
        return Officials::Collective {
            reason: "no name remained after titles were removed".into(),
        };
    }
    Officials::Individuals(names)
}

/// Remove the debris that arrives with text scraped out of court PDFs, then
/// normalise spacing and stray punctuation.
fn strip_extraction_artifacts(raw: &str) -> String {
    let mut s = raw.replace('\u{2019}', "'").replace(['\n', '\t'], " ");

    // "Form Field 44Cabret, Maria M." — the field label of a fillable PDF,
    // glued to the name that followed it.
    let lowered = s.to_lowercase();
    if let Some(at) = lowered.find("form field") {
        let rest = &s[at + "form field".len()..];
        let after_digits = rest
            .char_indices()
            .find(|(_, c)| !c.is_ascii_digit() && !c.is_whitespace())
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
        s = format!("{}{}", &s[..at], &rest[after_digits..]);
    }

    for label in [
        "before:",
        "panel:",
        "coram:",
        "judges:",
        "judge:",
        "opinion by:",
        "opinion by",
        "author:",
    ] {
        let lowered = s.to_lowercase();
        if lowered.trim_start().starts_with(label) {
            let at = lowered.find(label).expect("just matched");
            s = s[at + label.len()..].to_string();
        }
    }

    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|c: char| c != '.' && !c.is_alphanumeric())
        .to_string()
}

fn collective_marker(s: &str) -> Option<&'static str> {
    let norm: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    COLLECTIVE_MARKERS.iter().find(|m| norm == **m).copied()
}

/// Split on the separators that can only be separators: semicolons and
/// conjunctions. A comma is not one of them, so it is left to [`read_segment`].
fn split_on_separators(s: &str) -> Vec<String> {
    s.split(';')
        .flat_map(|part| {
            part.split('&').flat_map(|p| {
                // Split " and " as a word, so "Alexander" survives intact.
                let mut out = vec![String::new()];
                for token in p.split_whitespace() {
                    if token.eq_ignore_ascii_case("and") {
                        out.push(String::new());
                    } else {
                        let last = out.last_mut().expect("seeded with one element");
                        if !last.is_empty() {
                            last.push(' ');
                        }
                        last.push_str(token);
                    }
                }
                out
            })
        })
        .map(|p| p.trim().trim_matches(',').trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Read one separator-free segment, which may still hold commas.
///
/// Returns `None` when the commas could be read more than one way.
fn read_segment(segment: &str) -> Option<Vec<String>> {
    let pieces: Vec<&str> = segment
        .split(',')
        .map(str::trim)
        .filter(|p| !normalize_name(p).is_empty())
        .collect();

    if pieces.len() <= 1 {
        return Some(vec![clean_display_name(segment)]);
    }

    // A piece of one token is a bare surname; two or more is a full or given
    // name. That is the only signal a comma-separated list gives us.
    let word_counts: Vec<usize> = pieces
        .iter()
        .map(|p| normalize_name(p).split_whitespace().count())
        .collect();
    let all_surnames = word_counts.iter().all(|n| *n == 1);

    match (all_surnames, pieces.len()) {
        // Three bare surnames cannot be one name written surname-first, so
        // this is a panel: "Mathias, DeBoer, Kenworthy".
        (true, 3..) => Some(pieces.iter().map(|p| clean_display_name(p)).collect()),
        // "Smith, John" is unreadable: one judge surname-first, or two judges.
        (true, _) => None,
        // "Cabret, Maria M." is one judge. Put the name back in reading order
        // so it matches the same judge written "Maria M. Cabret".
        (false, 2) if word_counts[0] == 1 => Some(vec![clean_display_name(&format!(
            "{} {}",
            pieces[1], pieces[0]
        ))]),
        _ => None,
    }
}

/// Drop titles and post-nominals so the stored name is the person's name.
pub fn clean_display_name(raw: &str) -> String {
    let mut tokens: Vec<&str> = raw.split_whitespace().collect();

    let is_title = |tok: &str| {
        let bare: String = tok
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        TITLE_PREFIXES.contains(&bare.as_str())
            || matches!(
                bare.as_str(),
                "judge" | "judges" | "justice" | "hon" | "honorable"
            )
    };
    while tokens.first().is_some_and(|t| is_title(t)) {
        tokens.remove(0);
    }
    // Trailing reporter-style post-nominals: "Smith J.", "Mullins, JJ."
    while tokens.last().is_some_and(|t| {
        let bare: String = t
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        matches!(bare.as_str(), "j" | "jj" | "cj" | "js")
    }) {
        tokens.pop();
    }

    tokens
        .join(" ")
        .trim()
        .trim_matches(|c: char| c == ',' || c == ';')
        .trim()
        .to_string()
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

/// Recompute every actor's normalized name and fingerprint under the current
/// rules, merging any identities that turn out to be the same person.
///
/// Normalization rules improve — the day "Associate Judge Easterly" started
/// normalising to "easterly" was the day it stopped being a different person
/// from "Easterly". Without this pass those two rows stay separate forever, and
/// an individual's record stays split across them, which is an individual
/// nobody is accountable for.
///
/// Merging repoints every reference before deleting the duplicate, so no
/// finding, flag, link, package, or approval is lost.
pub async fn renormalize(pool: &PgPool, ledger: &Ledger) -> Result<serde_json::Value, Error> {
    let actors = sqlx::query_as::<_, Actor>(
        "SELECT actor_id, role, display_name, normalized_name, office, jurisdiction,
                bar_number, badge_number, prosecutor_id, fingerprint
         FROM accountability_actors
         -- An actor carrying a prosecutor id is the survivor of any merge, so
         -- settle those first and let the rest fold into them.
         ORDER BY (prosecutor_id IS NULL), actor_id",
    )
    .fetch_all(pool)
    .await?;

    let mut renamed = 0u64;
    let mut merged = Vec::new();

    for actor in actors {
        // The row may already have been merged away by an earlier iteration.
        if get(pool, actor.actor_id).await.is_err() {
            continue;
        }

        let display = clean_display_name(&actor.display_name);
        let display = if display.is_empty() {
            actor.display_name.clone()
        } else {
            display
        };
        let norm = normalize_name(&display);
        if norm.is_empty() {
            continue;
        }
        let fp = fingerprint(
            &actor.role,
            &norm,
            &actor.jurisdiction,
            actor.bar_number.as_deref(),
            actor.badge_number.as_deref(),
        );
        if fp == actor.fingerprint && display == actor.display_name {
            continue;
        }

        let survivor = sqlx::query_scalar::<_, Uuid>(
            "SELECT actor_id FROM accountability_actors
              WHERE fingerprint = $1 AND actor_id <> $2 LIMIT 1",
        )
        .bind(&fp)
        .bind(actor.actor_id)
        .fetch_optional(pool)
        .await?;

        match survivor {
            Some(into) => {
                merge_into(pool, actor.actor_id, into).await?;
                merged.push(json!({
                    "merged": actor.actor_id,
                    "into": into,
                    "display_name": display,
                }));
                ledger
                    .append(
                        vi_ledger::events::ACTOR_RESOLVED,
                        &json!({
                            "actor_id": into,
                            "absorbed": actor.actor_id,
                            "fingerprint": fp,
                            "method": "renormalize_merge",
                        }),
                    )
                    .await?;
            }
            None => {
                sqlx::query(
                    "UPDATE accountability_actors
                        SET display_name = $2, normalized_name = $3, fingerprint = $4
                      WHERE actor_id = $1",
                )
                .bind(actor.actor_id)
                .bind(&display)
                .bind(&norm)
                .bind(&fp)
                .execute(pool)
                .await?;
                // The name the record originally used stays searchable.
                sqlx::query(
                    "INSERT INTO actor_aliases (actor_id, alias, source, confidence)
                     VALUES ($1,$2,'renormalize',1.0) ON CONFLICT DO NOTHING",
                )
                .bind(actor.actor_id)
                .bind(&actor.display_name)
                .execute(pool)
                .await?;
                renamed += 1;
            }
        }
    }

    Ok(json!({
        "renamed": renamed,
        "merged": merged,
        "note": "Merging repoints findings, flags, links, packages, and approvals \
                 before the duplicate identity is removed.",
    }))
}

/// Move everything attached to `from` onto `into`, then delete `from`.
async fn merge_into(pool: &PgPool, from: Uuid, into: Uuid) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    for stmt in [
        "UPDATE constitutional_findings SET actor_id = $2 WHERE actor_id = $1",
        "UPDATE abuse_flags SET actor_id = $2 WHERE actor_id = $1",
        "UPDATE legal_action_packages SET actor_id = $2 WHERE actor_id = $1",
    ] {
        sqlx::query(stmt)
            .bind(from)
            .bind(into)
            .execute(&mut *tx)
            .await?;
    }

    // These carry uniqueness constraints, so a row that would collide is
    // dropped rather than moved: the surviving identity already has it.
    for stmt in [
        "UPDATE actor_case_links SET actor_id = $2 WHERE actor_id = $1
           AND NOT EXISTS (SELECT 1 FROM actor_case_links x
                            WHERE x.actor_id = $2 AND x.case_id = actor_case_links.case_id
                              AND x.role_in_case = actor_case_links.role_in_case)",
        "UPDATE actor_aliases SET actor_id = $2 WHERE actor_id = $1
           AND NOT EXISTS (SELECT 1 FROM actor_aliases x
                            WHERE x.actor_id = $2 AND x.alias = actor_aliases.alias)",
    ] {
        sqlx::query(stmt)
            .bind(from)
            .bind(into)
            .execute(&mut *tx)
            .await?;
    }

    // An approval is a human decision about a named individual. Carry it over
    // only if the survivor has none, and never downgrade one that exists.
    sqlx::query(
        "INSERT INTO publication_approvals
             (actor_id, approved, approved_by, approved_at, notes)
         SELECT $2, approved, approved_by, approved_at, notes
           FROM publication_approvals WHERE actor_id = $1
         ON CONFLICT (actor_id) DO NOTHING",
    )
    .bind(from)
    .bind(into)
    .execute(&mut *tx)
    .await?;

    // Score snapshots belong to the identity that produced them; the surviving
    // actor's score is recomputed from primary records anyway.
    sqlx::query("DELETE FROM accountability_actors WHERE actor_id = $1")
        .bind(from)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
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
    for (raw, jurisdiction) in judges {
        // A judge field naming a panel names several individuals. Only the
        // readable ones become identities; the rest are left for a human.
        let Officials::Individuals(names) = parse_officials(&raw) else {
            continue;
        };
        for name in names {
            let hit = resolve(
                pool,
                ledger,
                &ResolveQuery {
                    role: "judge".into(),
                    name,
                    jurisdiction: jurisdiction.clone(),
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
    fn strips_leading_titles_so_one_judge_is_one_actor() {
        assert_eq!(normalize_name("Associate Judge Easterly"), "easterly");
        assert_eq!(normalize_name("Easterly"), "easterly");
        assert_eq!(normalize_name("Chief Justice John Roberts"), "john roberts");
        assert_eq!(normalize_name("Judge Minor"), "minor");
    }

    /// Every string below came off a live CourtListener feed.
    #[test]
    fn splits_panels_into_individuals() {
        assert_eq!(
            parse_officials("Mathias, DeBoer, Kenworthy"),
            Officials::Individuals(vec!["Mathias".into(), "DeBoer".into(), "Kenworthy".into()])
        );
        assert_eq!(
            parse_officials("Bradford, Pyle III, Kenworthy"),
            Officials::Individuals(vec![
                "Bradford".into(),
                "Pyle III".into(),
                "Kenworthy".into()
            ])
        );
        assert_eq!(
            parse_officials("David W. McKeague; Joan L. Larsen; Kevin G. Ritz"),
            Officials::Individuals(vec![
                "David W. McKeague".into(),
                "Joan L. Larsen".into(),
                "Kevin G. Ritz".into()
            ])
        );
        assert_eq!(
            parse_officials("Mullins; McDonald; D\u{2019}Auria; Ecker; Dannehy; Bright"),
            Officials::Individuals(vec![
                "Mullins".into(),
                "McDonald".into(),
                "D'Auria".into(),
                "Ecker".into(),
                "Dannehy".into(),
                "Bright".into()
            ])
        );
    }

    #[test]
    fn a_panel_member_resolves_to_the_same_name_across_panels() {
        let one = parse_officials("Mathias, DeBoer, Kenworthy");
        let two = parse_officials("Bradford, Pyle III, Kenworthy");
        for panel in [&one, &two] {
            let Officials::Individuals(names) = panel else {
                panic!("expected individuals, got {panel:?}");
            };
            assert!(names.iter().any(|n| normalize_name(n) == "kenworthy"));
        }
    }

    #[test]
    fn drops_pdf_form_field_debris() {
        assert_eq!(
            parse_officials("Form Field 44Cabret, Maria M."),
            Officials::Individuals(vec!["Maria M. Cabret".into()])
        );
    }

    #[test]
    fn surname_first_is_reordered_so_it_matches_reading_order() {
        let inverted = parse_officials("Cabret, Maria M.");
        let plain = parse_officials("Maria M. Cabret");
        assert_eq!(inverted, plain);
    }

    #[test]
    fn single_names_keep_working() {
        assert_eq!(
            parse_officials("Smith J."),
            Officials::Individuals(vec!["Smith".into()])
        );
        assert_eq!(
            parse_officials("Judge Rudolph Contreras"),
            Officials::Individuals(vec!["Rudolph Contreras".into()])
        );
        assert_eq!(
            parse_officials("Associate Judge Easterly"),
            Officials::Individuals(vec!["Easterly".into()])
        );
    }

    #[test]
    fn refuses_to_guess_an_unreadable_pair() {
        // One judge surname-first, or two judges. Attributing either reading
        // risks putting one official's conduct on another's record.
        assert!(matches!(
            parse_officials("Smith, John"),
            Officials::Ambiguous { .. }
        ));
        assert!(matches!(
            parse_officials("Smith, John A., Jones"),
            Officials::Ambiguous { .. }
        ));
    }

    #[test]
    fn the_court_acting_as_a_body_is_not_an_individual() {
        assert!(matches!(
            parse_officials("Per Curiam"),
            Officials::Collective { .. }
        ));
        assert!(matches!(
            parse_officials("  "),
            Officials::Collective { .. }
        ));
        assert!(matches!(
            parse_officials("Judge"),
            Officials::Collective { .. }
        ));
    }

    #[test]
    fn conjunctions_separate_but_do_not_split_names() {
        assert_eq!(
            parse_officials("Roberts and Alexander"),
            Officials::Individuals(vec!["Roberts".into(), "Alexander".into()])
        );
        assert_eq!(
            parse_officials("Alexander"),
            Officials::Individuals(vec!["Alexander".into()])
        );
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
