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
    pub conditions: Vec<String>,
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
    conditions: &[String],
) -> Result<Session> {
    close_stale(db)?;
    let ts = now();
    let conditions = serde_json::to_string(
        &crate::engine::conditions::normalize(conditions)
            .iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
    )?;
    let id = db.with(|c| {
        c.execute(
            "INSERT INTO sessions(appid,started_at,budget,mood,appetite,hand_id,conditions)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![appid, ts, budget, mood, appetite, hand_id, conditions],
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
            "SELECT id, appid, started_at, budget, mood, appetite, first_impression, conditions
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
                    r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                ))
            },
        )
        .optional()?)
    })?;
    let Some((id, appid, started_at, budget, mood, appetite, fi_done, conditions)) = row else {
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
        conditions: serde_json::from_str(&conditions).unwrap_or_default(),
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

/// Why a session ended badly. The reason matters more than the bail: "I could
/// not hear it" says nothing about whether the game is good.
pub const BAIL_REASONS: &[(&str, &str)] = &[
    ("sound", "Needed the sound"),
    ("long", "Too long for tonight"),
    ("mood", "Wrong mood"),
    ("bad", "Just not good"),
];

pub fn reason_label(id: &str) -> String {
    BAIL_REASONS
        .iter()
        .find(|(k, _)| *k == id)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| id.to_string())
}

/// The verdicts a session may end with. Anything else is a typo, not a judgement.
const VERDICTS: &[&str] = &["loved", "kept", "fine", "bounced", "never"];

/// Ends a session with a verdict. Bailing out is a first-class, guilt-free outcome
/// and teaches the model just as much as loving it does, provided it knows why.
/// Ending a session twice, or one that was already closed as abandoned, changes
/// nothing: a verdict is only ever learned from once.
pub fn end(db: &Db, id: i64, verdict: &str, reason: Option<&str>) -> Result<i64> {
    if !VERDICTS.contains(&verdict) {
        return Err(anyhow!("unknown verdict “{verdict}”"));
    }
    let reason = reason
        .filter(|_| verdict == "bounced")
        .filter(|r| BAIL_REASONS.iter().any(|(k, _)| k == r));
    let (appid, conditions, closed) = db.with(|c| {
        let row: (i64, Option<String>) = c.query_row(
            "SELECT appid, conditions FROM sessions WHERE id=?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let closed = c.execute(
            "UPDATE sessions SET ended_at=?2, verdict=?3, bail_reason=?4
             WHERE id=?1 AND ended_at IS NULL",
            params![id, now(), verdict, reason],
        )?;
        Ok((row.0, row.1, closed))
    })?;
    if closed == 0 {
        return Ok(appid);
    }
    let conditions: Vec<String> = conditions
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    let seconds = db.with(|c| {
        Ok(c.query_row(
            "SELECT COALESCE(ended_at,0) - started_at FROM sessions WHERE id=?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0))
    })?;

    let blames_the_game = reason.is_none() || reason == Some("bad");
    db.log_event(
        appid,
        if blames_the_game { verdict } else { "bailed" },
        None,
        None,
        None,
        Some(seconds),
    )?;

    let mut signal = taste::signal_for(verdict);
    if let Some(why) = reason {
        db.log_note(appid, "bail_reason", why)?;
        if why != "bad" {
            signal = -0.1;
        }
        if why == "sound" {
            crate::engine::overrides::set(db, appid, "sound", Some(0.85))?;
        }
    }
    if matches!(verdict, "loved" | "kept" | "fine") && conditions.iter().any(|c| c == "quiet") {
        db.log_event(appid, "quiet_worked", None, None, None, None)?;
    }
    taste::nudge(db, appid, signal)?;
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
    pub reason: Option<String>,
    pub conditions: Vec<String>,
}

pub fn recent(db: &Db, limit: usize) -> Result<Vec<HistoryRow>> {
    db.with(|c| {
        let mut st = c.prepare(
            "SELECT s.appid, COALESCE(NULLIF(g.name,''), m.name, 'App ' || s.appid), s.started_at,
                    (COALESCE(s.ended_at, s.started_at) - s.started_at)/60,
                    COALESCE(s.verdict,'open'), s.bail_reason, s.conditions
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
                reason: r.get::<_, Option<String>>(5)?.map(|id| reason_label(&id)),
                conditions: r
                    .get::<_, Option<String>>(6)?
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::tests::Scratch;

    fn seed(db: &Db, appid: i64, name: &str, tags: &str) {
        db.with(|c| {
            c.execute(
                "INSERT INTO games(appid,name,playtime_min,installed,first_seen,updated_at)
                 VALUES(?1,?2,0,1,1,1)",
                params![appid, name],
            )?;
            c.execute(
                "INSERT INTO meta(appid,kind,tags,fetched_at,ok) VALUES(?1,'game',?2,1,1)",
                params![appid, tags],
            )?;
            Ok(())
        })
        .unwrap();
    }

    fn sound_of(db: &Db, appid: i64) -> f64 {
        load::one(db, appid).unwrap().unwrap().shape.sound.value
    }

    #[test]
    fn bailing_because_the_sound_was_needed_pins_that_game_as_sound_hungry() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 10, "Quiet Looking", r#"["Turn-Based Strategy"]"#);
        let before = sound_of(db, 10);
        let s = start(
            db,
            10,
            None,
            45,
            "mellow",
            "surprise",
            &["quiet".to_string()],
        )
        .unwrap();
        end(db, s.id, "bounced", Some("sound")).unwrap();
        let after = sound_of(db, 10);
        assert!(before < 0.4, "got {before}");
        assert!((after - 0.85).abs() < 1e-9, "got {after}");
        assert_eq!(
            recent(db, 1).unwrap()[0].reason.as_deref(),
            Some("Needed the sound")
        );
    }

    #[test]
    fn a_bail_for_mood_barely_dents_taste() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 11, "Mood", r#"["Puzzle"]"#);
        let s = start(db, 11, None, 0, "mellow", "surprise", &[]).unwrap();
        end(db, s.id, "bounced", Some("mood")).unwrap();
        let weight: f64 = db
            .with(|c| {
                Ok(
                    c.query_row("SELECT weight FROM taste WHERE dim='tag:Puzzle'", [], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert!(weight > -0.1, "got {weight}");
    }

    #[test]
    fn a_good_night_with_the_sound_taken_is_remembered() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 12, "Worked", r#"["Atmospheric", "Great Soundtrack"]"#);
        let before = sound_of(db, 12);
        let s = start(
            db,
            12,
            None,
            45,
            "mellow",
            "surprise",
            &["quiet".to_string()],
        )
        .unwrap();
        end(db, s.id, "loved", None).unwrap();
        let after = sound_of(db, 12);
        assert!(after < before - 0.1, "before {before} after {after}");
    }

    #[test]
    fn an_unknown_reason_is_ignored_and_a_game_is_never_retired_by_a_bail() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 13, "Odd", "[]");
        let s = start(db, 13, None, 0, "mellow", "surprise", &[]).unwrap();
        end(db, s.id, "bounced", Some("because")).unwrap();
        assert_eq!(recent(db, 1).unwrap()[0].reason, None);
        assert!(!load::one(db, 13).unwrap().unwrap().prefs.amnesty);
    }

    #[test]
    fn only_one_session_is_ever_open() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 14, "A", "[]");
        seed(db, 15, "B", "[]");
        start(db, 14, None, 0, "mellow", "surprise", &[]).unwrap();
        let second = start(db, 15, None, 0, "mellow", "surprise", &[]).unwrap();
        assert_eq!(current(db).unwrap().unwrap().id, second.id);
    }

    #[test]
    fn ending_twice_learns_once() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 20, "Once", r#"["Puzzle"]"#);
        let s = start(db, 20, None, 0, "mellow", "surprise", &[]).unwrap();
        end(db, s.id, "loved", None).unwrap();
        end(db, s.id, "loved", None).unwrap();
        let loved = load::one(db, 20).unwrap().unwrap().history.loved;
        let weight: f64 = db
            .with(|c| {
                Ok(
                    c.query_row("SELECT weight FROM taste WHERE dim='tag:Puzzle'", [], |r| {
                        r.get(0)
                    })?,
                )
            })
            .unwrap();
        assert_eq!(loved, 1);
        assert!(weight < 0.5, "taste moved twice: {weight}");
    }

    #[test]
    fn an_abandoned_session_closed_by_a_new_one_cannot_be_judged_later() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 21, "Old", "[]");
        seed(db, 22, "New", "[]");
        let old = start(db, 21, None, 0, "mellow", "surprise", &[]).unwrap();
        start(db, 22, None, 0, "mellow", "surprise", &[]).unwrap();
        end(db, old.id, "never", None).unwrap();
        assert!(!load::one(db, 21).unwrap().unwrap().prefs.amnesty);
    }

    #[test]
    fn nonsense_verdicts_are_refused_and_reasons_only_follow_a_bail() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 23, "Strict", "[]");
        let s = start(db, 23, None, 0, "mellow", "surprise", &[]).unwrap();
        assert!(end(db, s.id, "banana", None).is_err());
        end(db, s.id, "loved", Some("sound")).unwrap();
        assert_eq!(recent(db, 1).unwrap()[0].reason, None);
        assert!(load::one(db, 23).unwrap().unwrap().prefs.traits.is_empty());
    }

    #[test]
    fn a_bail_for_a_reason_other_than_the_game_is_not_counted_against_it() {
        let scratch = Scratch::new();
        let db = &scratch.db;
        seed(db, 24, "Innocent", "[]");
        for reason in ["mood", "long", "sound"] {
            let s = start(db, 24, None, 0, "mellow", "surprise", &[]).unwrap();
            end(db, s.id, "bounced", Some(reason)).unwrap();
        }
        let h = load::one(db, 24).unwrap().unwrap().history;
        assert_eq!(h.bounces, 0);
        let s = start(db, 24, None, 0, "mellow", "surprise", &[]).unwrap();
        end(db, s.id, "bounced", Some("bad")).unwrap();
        assert_eq!(load::one(db, 24).unwrap().unwrap().history.bounces, 1);
    }
}
