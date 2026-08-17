use crate::db::{now, Db};
use crate::engine::{load, taste};
use anyhow::{anyhow, Result};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Session {
    pub id: i64,
    pub appid: i64,
    pub name: String,
    pub started_at: i64,
    pub elapsed: i64,
    pub first_impression_at: i64,
    pub first_impression_done: bool,
    pub budget: i64,
    pub mood: String,
    pub appetite: String,
    pub session_label: String,
    pub launch_url: String,
    pub can_launch_here: bool,
}

fn first_impression_minutes(db: &Db) -> i64 {
    db.setting_i64("first_impression_minutes", 12).clamp(1, 120)
}

/// Starts the commitment: one game, one clock, and a scheduled honesty check.
pub fn start(
    db: &Db,
    appid: i64,
    hand_id: Option<&str>,
    budget: i64,
    mood: &str,
    appetite: &str,
) -> Result<Session> {
    close_stale(db)?;
    let ts = now();
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO sessions(appid,started_at,budget,mood,appetite,hand_id)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![appid, ts, budget, mood, appetite, hand_id],
        )?;
        Ok(c.last_insert_rowid())
    })?;
    if let Some(h) = hand_id {
        db.with(|c| {
            c.execute(
                "UPDATE hands SET resolved=?2 WHERE id=?1",
                params![h, appid],
            )?;
            Ok(())
        })?;
    }
    db.log_event(
        appid,
        "chosen",
        Some(budget),
        Some(mood),
        Some(appetite),
        None,
    )?;
    taste::nudge(db, appid, taste::signal_for("chosen"))?;
    current(db).and_then(|s| s.ok_or_else(|| anyhow!("session {id} vanished")))
}

pub fn current(db: &Db) -> Result<Option<Session>> {
    let fi = first_impression_minutes(db);
    let allow_launch = db.setting_bool("allow_launch", false);
    let row = db.with(|c| {
        Ok(c.query_row(
            "SELECT id, appid, started_at, budget, mood, appetite, first_impression
             FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                    r.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?)
    })?;
    let Some((id, appid, started_at, budget, mood, appetite, fi_done)) = row else {
        return Ok(None);
    };
    let cand = load::one(db, appid)?;
    Ok(Some(Session {
        id,
        appid,
        name: cand
            .as_ref()
            .map(|c| c.game.name.clone())
            .unwrap_or_else(|| load::name_of(db, appid)),
        started_at,
        elapsed: (now() - started_at).max(0),
        first_impression_at: started_at + fi * 60,
        first_impression_done: fi_done.is_some(),
        budget,
        mood,
        appetite,
        session_label: cand
            .map(|c| c.shape.session_label)
            .unwrap_or_else(|| "an hour or so".into()),
        launch_url: format!("steam://rungameid/{appid}"),
        can_launch_here: allow_launch,
    }))
}

pub fn first_impression(db: &Db, id: i64, answer: &str) -> Result<()> {
    db.with(|c| {
        c.execute(
            "UPDATE sessions SET first_impression=?2 WHERE id=?1",
            params![id, answer],
        )?;
        Ok(())
    })?;
    Ok(())
}

/// Ends a session with a verdict. Bailing out is a first-class, guilt-free outcome
/// and teaches the model just as much as loving it does.
pub fn end(db: &Db, id: i64, verdict: &str) -> Result<i64> {
    let appid = db.with(|c| {
        let appid: i64 =
            c.query_row("SELECT appid FROM sessions WHERE id=?1", params![id], |r| {
                r.get(0)
            })?;
        c.execute(
            "UPDATE sessions SET ended_at=?2, verdict=?3 WHERE id=?1 AND ended_at IS NULL",
            params![id, now(), verdict],
        )?;
        Ok(appid)
    })?;
    let seconds = db.with(|c| {
        Ok(c.query_row(
            "SELECT COALESCE(ended_at,0) - started_at FROM sessions WHERE id=?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0))
    })?;
    db.log_event(appid, verdict, None, None, None, Some(seconds))?;
    taste::nudge(db, appid, taste::signal_for(verdict))?;
    if verdict == "never" {
        crate::engine::reckoning::set_amnesty(db, appid, true)?;
    }
    Ok(appid)
}

/// Only one game can be on the clock. Anything still open when a new session
/// starts was abandoned at the keyboard, so it is closed without a verdict and
/// left out of the statistics.
fn close_stale(db: &Db) -> Result<()> {
    db.with(|c| {
        c.execute(
            "UPDATE sessions SET ended_at = started_at, verdict='unknown'
             WHERE ended_at IS NULL",
            [],
        )?;
        Ok(())
    })
}

#[derive(Debug, Serialize)]
pub struct HistoryRow {
    pub appid: i64,
    pub name: String,
    pub started_at: i64,
    pub minutes: i64,
    pub verdict: String,
}

pub fn recent(db: &Db, limit: usize) -> Result<Vec<HistoryRow>> {
    db.with(|c| {
        let mut st = c.prepare(
            "SELECT s.appid, COALESCE(NULLIF(g.name,''), m.name, 'App ' || s.appid), s.started_at,
                    (COALESCE(s.ended_at, s.started_at) - s.started_at)/60,
                    COALESCE(s.verdict,'open')
             FROM sessions s
             LEFT JOIN games g ON g.appid=s.appid
             LEFT JOIN meta  m ON m.appid=s.appid
             ORDER BY s.started_at DESC LIMIT ?1",
        )?;
        let rows = st.query_map(params![limit as i64], |r| {
            Ok(HistoryRow {
                appid: r.get(0)?,
                name: r.get(1)?,
                started_at: r.get(2)?,
                minutes: r.get(3)?,
                verdict: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
}
