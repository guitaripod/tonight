use crate::db::{now, Db};
use crate::engine::{load, score};
use crate::model::Candidate;
use anyhow::Result;
use rusqlite::params;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Named {
    pub appid: i64,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Reckoning {
    pub owned: i64,
    pub untouched: i64,
    pub untouched_pct: f64,
    pub barely: i64,
    pub finished_ish: i64,
    pub hours_played: f64,
    pub backlog_hours: f64,
    pub hours_per_week: f64,
    pub years_to_clear: f64,
    pub finish_year: i64,
    pub money_untouched: f64,
    pub currency: String,
    pub ghosts: i64,
    pub amnesty_count: i64,
    pub amnesty_hours_freed: f64,
    pub sessions_logged: i64,
    pub hours_logged: f64,
    pub bounce_rate: f64,
    pub rescued: Vec<Named>,
    pub oldest_ghost: Option<Named>,
    pub most_passed: Option<Named>,
    pub biggest_love: Option<Named>,
    pub top_archetypes: Vec<(String, i64)>,
    pub verdict: String,
    pub coverage_note: Option<String>,
}

fn years_phrase(y: f64) -> String {
    if y > 500.0 {
        "never".into()
    } else {
        format!("{:.0}", y)
    }
}

/// The honest arithmetic of a backlog: how much of it you own, how little of it
/// you have touched, and the year you would finish at your actual pace.
pub fn compute(db: &Db) -> Result<Reckoning> {
    let all = load::candidates(db)?;
    let live: Vec<&Candidate> = all.iter().filter(|c| !c.prefs.amnesty).collect();
    let real: Vec<&&Candidate> = live
        .iter()
        .filter(|c| !c.game.name.trim().is_empty())
        .collect();

    let owned = real.len() as i64;
    let untouched = real.iter().filter(|c| c.game.playtime_min == 0).count() as i64;
    let barely = real
        .iter()
        .filter(|c| c.game.playtime_min > 0 && c.game.playtime_min < 60)
        .count() as i64;
    let finished_ish = real
        .iter()
        .filter(|c| {
            c.shape.completion_hours > 0.0
                && (c.game.playtime_min as f64 / 60.0) >= c.shape.completion_hours * 0.8
        })
        .count() as i64;
    let hours_played: f64 = real.iter().map(|c| c.game.playtime_min as f64 / 60.0).sum();

    let backlog_hours: f64 = real
        .iter()
        .filter(|c| c.shape.completion_hours < 300.0)
        .map(|c| {
            let done = c.game.playtime_min as f64 / 60.0;
            (c.shape.completion_hours - done).max(0.0)
        })
        .sum();

    let two_week_hours: f64 = real.iter().map(|c| c.game.playtime_2w as f64 / 60.0).sum();
    let configured = db
        .setting("hours_per_week")
        .and_then(|v| v.parse::<f64>().ok());
    let hours_per_week = configured
        .or_else(|| (two_week_hours > 0.1).then(|| two_week_hours / 2.0))
        .unwrap_or(6.0)
        .max(0.25);
    let years_to_clear = backlog_hours / (hours_per_week * 52.0);
    let finish_year = 1970 + ((now() as f64 / 31_557_600.0) + years_to_clear) as i64;

    let money_untouched: f64 = real
        .iter()
        .filter(|c| c.game.playtime_min == 0)
        .map(|c| c.meta.price_initial as f64 / 100.0)
        .sum();
    let currency = real
        .iter()
        .find_map(|c| (!c.meta.currency.is_empty()).then(|| c.meta.currency.clone()))
        .unwrap_or_else(|| "EUR".into());

    let two_years = now() - 2 * 365 * 86400;
    let ghosts = real
        .iter()
        .filter(|c| {
            c.game.playtime_min == 0 && c.meta.release_year > 0 && c.meta.release_year < 2100
        })
        .filter(|c| c.game.last_played == 0 || c.game.last_played < two_years)
        .count() as i64;

    let amnesty: Vec<&Candidate> = all.iter().filter(|c| c.prefs.amnesty).collect();
    let amnesty_hours_freed: f64 = amnesty
        .iter()
        .filter(|c| c.shape.completion_hours < 300.0)
        .map(|c| (c.shape.completion_hours - c.game.playtime_min as f64 / 60.0).max(0.0))
        .sum();

    let (sessions_logged, hours_logged, bounces) = db.with(|c| {
        let s: (i64, f64) = c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(MAX(0, COALESCE(ended_at, started_at) - started_at))/3600.0, 0)
             FROM sessions WHERE COALESCE(verdict,'') <> 'unknown'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let b: i64 = c.query_row(
            "SELECT COUNT(*) FROM sessions WHERE verdict='bounced'",
            [],
            |r| r.get(0),
        )?;
        Ok((s.0, s.1, b))
    })?;
    let bounce_rate = if sessions_logged > 0 {
        bounces as f64 / sessions_logged as f64
    } else {
        0.0
    };

    let rescued = db.with(|c| {
        let mut st = c.prepare(
            "SELECT s.appid, COALESCE(NULLIF(g.name,''), 'App ' || s.appid), MIN(s.started_at)
             FROM sessions s JOIN games g ON g.appid = s.appid
             WHERE s.verdict IN ('loved','fine','kept')
             GROUP BY s.appid ORDER BY MIN(s.started_at) DESC LIMIT 8",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Named {
                appid: r.get(0)?,
                name: r.get(1)?,
                detail: score::ago(((now() - r.get::<_, i64>(2)?) as f64) / 86400.0),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })?;

    let oldest_ghost = real
        .iter()
        .filter(|c| c.game.playtime_min == 0 && c.meta.release_year > 1990)
        .min_by_key(|c| c.meta.release_year)
        .map(|c| Named {
            appid: c.game.appid,
            name: c.game.name.clone(),
            detail: format!("released {}, still sealed", c.meta.release_year),
        });

    let most_passed = real
        .iter()
        .filter(|c| c.history.passes > 0)
        .max_by_key(|c| c.history.passes)
        .map(|c| Named {
            appid: c.game.appid,
            name: c.game.name.clone(),
            detail: if c.history.passes == 1 {
                "passed on once".to_string()
            } else {
                format!("passed on {} times", c.history.passes)
            },
        });

    let biggest_love = real
        .iter()
        .max_by_key(|c| c.game.playtime_min)
        .map(|c| Named {
            appid: c.game.appid,
            name: c.game.name.clone(),
            detail: format!(
                "{} of your life",
                score::minutes_phrase(c.game.playtime_min)
            ),
        });

    let mut arch: std::collections::HashMap<String, i64> = Default::default();
    for c in real.iter().filter(|c| c.game.playtime_min > 30) {
        *arch.entry(c.shape.archetype.clone()).or_default() += c.game.playtime_min / 60;
    }
    let mut top_archetypes: Vec<(String, i64)> =
        arch.into_iter().filter(|(k, _)| k != "unknown").collect();
    top_archetypes.sort_by_key(|(_, v)| -v);
    top_archetypes.truncate(6);

    let verdict = if owned == 0 {
        "Nothing synced yet. Point tonight at your Steam install and pull the library in.".into()
    } else if untouched as f64 / owned.max(1) as f64 > 0.5 {
        format!(
            "You have never opened {untouched} of your {owned} games. At {:.0} hours a week you would finish in {}. Amnesty is not failure — it is arithmetic.",
            hours_per_week,
            years_phrase(years_to_clear)
        )
    } else {
        format!(
            "{:.0} hours played, {:.0} hours left. You are further along than you think.",
            hours_played, backlog_hours
        )
    };

    let coverage_note = coverage_note(db, owned, untouched);

    Ok(Reckoning {
        owned,
        untouched,
        untouched_pct: if owned > 0 {
            untouched as f64 * 100.0 / owned as f64
        } else {
            0.0
        },
        barely,
        finished_ish,
        hours_played,
        backlog_hours,
        hours_per_week,
        years_to_clear,
        finish_year,
        money_untouched,
        currency,
        ghosts,
        amnesty_count: amnesty.len() as i64,
        amnesty_hours_freed,
        sessions_logged,
        hours_logged,
        bounce_rate,
        rescued,
        oldest_ghost,
        most_passed,
        biggest_love,
        top_archetypes,
        verdict,
        coverage_note,
    })
}

/// Without a Web API key the library is only what the Steam client has cached,
/// which skews heavily towards games you have already launched. Say so rather
/// than quietly reporting a flattering "never opened" count.
fn coverage_note(db: &Db, owned: i64, untouched: i64) -> Option<String> {
    let has_key = !db.setting_or("steam_api_key", "").trim().is_empty();
    if has_key || owned == 0 {
        return None;
    }
    let share = untouched as f64 / owned as f64;
    if share > 0.25 {
        return None;
    }
    Some(format!(
        "These {owned} games are what your Steam client has cached locally — mostly ones you have already launched. Add a Web API key in Settings to count everything you own but never installed."
    ))
}

#[derive(Debug, Serialize)]
pub struct AmnestyCard {
    pub appid: i64,
    pub name: String,
    pub why: String,
    pub hours: f64,
    pub genres: Vec<String>,
    pub release_year: i64,
}

/// Offers the games least likely to ever be played, so retiring them costs nothing.
pub fn amnesty_candidates(db: &Db, limit: usize) -> Result<Vec<AmnestyCard>> {
    let all = load::candidates(db)?;
    let mut pool: Vec<&Candidate> = all
        .iter()
        .filter(|c| !c.prefs.amnesty && !c.prefs.pinned && !c.game.name.trim().is_empty())
        .filter(|c| c.game.playtime_min < 45)
        .collect();

    pool.sort_by_key(|c| {
        let age = 2100 - c.meta.release_year.max(1990);
        let quality = c.meta.metacritic;
        let passes = c.history.passes;
        let touched = c.game.playtime_min;
        -(passes * 40 + age / 2 + (100 - quality.min(100)) + touched.min(45))
    });

    Ok(pool
        .into_iter()
        .take(limit)
        .map(|c| AmnestyCard {
            appid: c.game.appid,
            name: c.game.name.clone(),
            why: amnesty_reason(c),
            hours: (c.shape.completion_hours * 10.0).round() / 10.0,
            genres: c.meta.genres.clone(),
            release_year: c.meta.release_year,
        })
        .collect())
}

fn amnesty_reason(c: &Candidate) -> String {
    if c.history.passes >= 2 {
        format!(
            "You have passed on this {} times. That is an answer.",
            c.history.passes
        )
    } else if c.game.playtime_min > 0 {
        format!(
            "{} in, then nothing. {} ago.",
            score::minutes_phrase(c.game.playtime_min),
            score::ago(((now() - c.game.last_played).max(0) as f64) / 86400.0)
        )
    } else if c.meta.release_year > 1990 {
        format!("From {}. Never opened, not once.", c.meta.release_year)
    } else {
        "Never opened, and you cannot remember buying it.".into()
    }
}

pub fn set_amnesty(db: &Db, appid: i64, on: bool) -> Result<()> {
    db.with(|c| {
        c.execute(
            "INSERT INTO prefs(appid,amnesty,updated_at) VALUES(?1,?2,?3)
             ON CONFLICT(appid) DO UPDATE SET amnesty=?2, updated_at=?3",
            params![appid, on as i64, now()],
        )?;
        Ok(())
    })?;
    db.log_event(
        appid,
        if on { "amnesty" } else { "unamnesty" },
        None,
        None,
        None,
        None,
    )?;
    if on {
        crate::engine::taste::nudge(db, appid, -0.9)?;
    }
    Ok(())
}

pub fn set_pinned(db: &Db, appid: i64, on: bool) -> Result<()> {
    db.with(|c| {
        c.execute(
            "INSERT INTO prefs(appid,pinned,updated_at) VALUES(?1,?2,?3)
             ON CONFLICT(appid) DO UPDATE SET pinned=?2, updated_at=?3",
            params![appid, on as i64, now()],
        )?;
        Ok(())
    })?;
    db.log_event(
        appid,
        if on { "pinned" } else { "unpinned" },
        None,
        None,
        None,
        None,
    )
}
