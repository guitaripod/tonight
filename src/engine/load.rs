use crate::db::Db;
use crate::engine::shape;
use crate::model::{Candidate, Game, History, Meta, Prefs};
use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;

const NON_GAME_KINDS: &[&str] = &[
    "dlc",
    "music",
    "video",
    "series",
    "episode",
    "tool",
    "application",
    "hardware",
    "demo",
    "advertising",
    "mod",
    "unknown",
];

fn json_strings(s: Option<String>) -> Vec<String> {
    s.and_then(|v| serde_json::from_str::<Vec<String>>(&v).ok())
        .unwrap_or_default()
}

/// Joins two name lists without repeats, ignoring case, keeping `first`'s order.
/// The storefront and the local client spell a few genres differently.
fn union(first: Vec<String>, second: Vec<String>) -> Vec<String> {
    let mut out = first;
    for item in second {
        if !out.iter().any(|x| x.eq_ignore_ascii_case(&item)) {
            out.push(item);
        }
    }
    out
}

fn trait_overrides(db: &Db) -> Result<HashMap<i64, HashMap<String, f64>>> {
    db.with(|c| {
        let mut st = c.prepare("SELECT appid, trait, value FROM overrides")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })?;
        let mut out: HashMap<i64, HashMap<String, f64>> = HashMap::new();
        for row in rows {
            let (appid, name, value) = row?;
            out.entry(appid).or_default().insert(name, value);
        }
        Ok(out)
    })
}

/// Loads the entire library into memory with metadata, preferences, personal
/// history and derived shape attached. Cheap enough to redo on every deal.
pub fn candidates(db: &Db) -> Result<Vec<Candidate>> {
    let history = history_map(db)?;
    let observed = observed_sessions(db)?;
    let overrides = trait_overrides(db)?;
    db.with(|c| {
        let mut st = c.prepare(
            "SELECT g.appid, g.name, g.playtime_min, g.playtime_2w, g.last_played, g.installed,
                    g.size_bytes,
                    m.kind, m.short_desc, m.genres, m.categories, m.developers, m.release_year,
                    m.metacritic, m.recommendations, m.price_initial, m.currency,
                    m.achievements_total, m.fetched_at, m.name,
                    p.amnesty, p.pinned, p.session_minutes, p.elo, p.duels,
                    a.unlocked, a.total,
                    m.tags, m.l_genres, m.l_categories, m.l_metacritic, m.l_release_year,
                    m.audio_langs, m.audio_full, m.subtitles, m.deck, m.deck_input,
                    m.needs_online, m.review_pct
             FROM games g
             LEFT JOIN meta   m ON m.appid = g.appid
             LEFT JOIN prefs  p ON p.appid = g.appid
             LEFT JOIN achievements a ON a.appid = g.appid",
        )?;
        let rows = st.query_map([], |r| {
            let appid: i64 = r.get(0)?;
            let store_name: Option<String> = r.get(19)?;
            let local_name: String = r.get(1)?;
            let name = if local_name.trim().is_empty() {
                store_name.clone().unwrap_or_default()
            } else {
                local_name
            };
            let game = Game {
                appid,
                name,
                playtime_min: r.get(2)?,
                playtime_2w: r.get(3)?,
                last_played: r.get(4)?,
                installed: r.get::<_, i64>(5)? != 0,
                size_bytes: r.get(6)?,
            };
            let store_metacritic = r.get::<_, Option<i64>>(13)?.unwrap_or(0);
            let store_year = r.get::<_, Option<i64>>(12)?.unwrap_or(0);
            let local_metacritic = r.get::<_, Option<i64>>(30)?.unwrap_or(0);
            let local_year = r.get::<_, Option<i64>>(31)?.unwrap_or(0);
            let needs_online = r.get::<_, Option<i64>>(37)?.unwrap_or(-1);
            let meta = Meta {
                kind: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                short_desc: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                genres: union(json_strings(r.get(9)?), json_strings(r.get(28)?)),
                categories: union(json_strings(r.get(10)?), json_strings(r.get(29)?)),
                tags: json_strings(r.get(27)?),
                audio_langs: r.get::<_, Option<i64>>(32)?.unwrap_or(-1),
                audio_full: r.get::<_, Option<i64>>(33)?.unwrap_or(-1),
                subtitles: r.get::<_, Option<i64>>(34)?.unwrap_or(-1),
                deck: r.get::<_, Option<i64>>(35)?.unwrap_or(0),
                deck_input: r.get::<_, Option<String>>(36)?,
                needs_online: (needs_online >= 0).then_some(needs_online > 0),
                review_pct: r.get::<_, Option<i64>>(38)?.unwrap_or(0),
                developers: json_strings(r.get(11)?),
                release_year: if store_year > 0 {
                    store_year
                } else {
                    local_year
                },
                metacritic: if store_metacritic > 0 {
                    store_metacritic
                } else {
                    local_metacritic
                },
                recommendations: r.get::<_, Option<i64>>(14)?.unwrap_or(0),
                price_initial: r.get::<_, Option<i64>>(15)?.unwrap_or(0),
                currency: r.get::<_, Option<String>>(16)?.unwrap_or_default(),
                achievements_total: r.get::<_, Option<i64>>(17)?.unwrap_or(0),
                fetched_at: r.get::<_, Option<i64>>(18)?.unwrap_or(0),
            };
            let prefs = Prefs {
                amnesty: r.get::<_, Option<i64>>(20)?.unwrap_or(0) != 0,
                pinned: r.get::<_, Option<i64>>(21)?.unwrap_or(0) != 0,
                session_override: r.get::<_, Option<i64>>(22)?,
                elo: r.get::<_, Option<f64>>(23)?.unwrap_or(1500.0),
                duels: r.get::<_, Option<i64>>(24)?.unwrap_or(0),
                traits: overrides.get(&appid).cloned().unwrap_or_default(),
            };
            let ach = (
                r.get::<_, Option<i64>>(25)?.unwrap_or(0),
                r.get::<_, Option<i64>>(26)?.unwrap_or(0),
            );
            Ok((game, meta, prefs, ach))
        })?;

        let mut out = vec![];
        for row in rows {
            let (game, meta, prefs, ach) = row?;
            let mut hist = history.get(&game.appid).cloned().unwrap_or_default();
            hist.ach_unlocked = ach.0;
            hist.ach_total = if ach.1 > 0 {
                ach.1
            } else {
                meta.achievements_total
            };
            hist.observed_session_minutes = observed.get(&game.appid).copied();
            let shape = shape::derive(
                &game,
                &meta,
                &shape::Personal {
                    session_override: prefs.session_override,
                    observed_session: hist.observed_session_minutes,
                    traits: Some(&prefs.traits),
                    quiet_worked: hist.quiet_worked,
                },
            );
            out.push(Candidate {
                game,
                meta,
                prefs,
                shape,
                history: hist,
            });
        }
        Ok(out)
    })
}

const SOFTWARE_GENRES: &[&str] = &[
    "Utilities",
    "Design & Illustration",
    "Animation & Modeling",
    "Video Production",
    "Audio Production",
    "Photo Editing",
    "Web Publishing",
    "Software Training",
    "Game Development",
    "Accounting",
];

/// Steam files creative tools and wallpaper apps as games. They are not evenings.
fn is_software(c: &Candidate) -> bool {
    c.meta
        .genres
        .iter()
        .any(|g| SOFTWARE_GENRES.contains(&g.as_str()))
}

/// Whether an entry is a game at all: named, not a demo or a tool, not creative
/// software that Steam files under games.
pub fn is_game(c: &Candidate) -> bool {
    let k = c.meta.kind.to_ascii_lowercase();
    !is_software(c)
        && !c.game.name.trim().is_empty()
        && (k.is_empty() || !NON_GAME_KINDS.contains(&k.as_str()))
}

/// Games eligible to be dealt: real games, not retired, not sworn off.
pub fn eligible(all: &[Candidate]) -> Vec<&Candidate> {
    all.iter()
        .filter(|c| !c.prefs.amnesty)
        .filter(|c| is_game(c))
        .filter(|c| c.history.last_verdict != "never")
        .collect()
}

fn history_map(db: &Db) -> Result<HashMap<i64, History>> {
    db.with(|c| {
        let mut out: HashMap<i64, History> = HashMap::new();
        let mut st =
            c.prepare("SELECT appid, kind, COUNT(*), MAX(ts) FROM events GROUP BY appid, kind")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (appid, kind, count, last) = row?;
            let h = out.entry(appid).or_default();
            match kind.as_str() {
                "passed" => {
                    h.passes = count;
                    h.last_pass = last;
                }
                "launched" | "chosen" => {
                    h.launches += count;
                }
                "bounced" => h.bounces = count,
                "loved" => h.loved = count,
                "quiet_worked" => h.quiet_worked = count,
                "dealt" => h.last_dealt = last,
                _ => {}
            }
        }
        let mut vst = c.prepare(
            "SELECT appid, kind FROM events
             WHERE kind IN ('loved','fine','never','bounced','kept')
             ORDER BY ts ASC",
        )?;
        let vrows = vst.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for row in vrows {
            let (appid, kind) = row?;
            out.entry(appid).or_default().last_verdict = kind;
        }
        Ok(out)
    })
}

/// The player's own median sitting length per game, in minutes, from sessions
/// this app timed. Beats any genre heuristic once three sessions exist.
fn observed_sessions(db: &Db) -> Result<HashMap<i64, i64>> {
    db.with(|c| {
        let mut st = c.prepare(
            "SELECT appid, (ended_at - started_at)/60 AS mins FROM sessions
             WHERE ended_at IS NOT NULL AND ended_at > started_at
               AND COALESCE(verdict,'') <> 'unknown'
               AND (ended_at - started_at) <= 8 * 3600
             ORDER BY appid, mins",
        )?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        let mut buckets: HashMap<i64, Vec<i64>> = HashMap::new();
        for row in rows {
            let (appid, mins) = row?;
            buckets.entry(appid).or_default().push(mins);
        }
        Ok(buckets
            .into_iter()
            .filter(|(_, v)| v.len() >= 2)
            .map(|(k, v)| (k, v[v.len() / 2]))
            .collect())
    })
}

pub fn taste_map(db: &Db) -> Result<HashMap<String, f64>> {
    db.with(|c| {
        let mut st = c.prepare("SELECT dim, weight FROM taste")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))?;
        let mut out = HashMap::new();
        for row in rows {
            let (k, v) = row?;
            out.insert(k, v);
        }
        Ok(out)
    })
}

pub fn one(db: &Db, appid: i64) -> Result<Option<Candidate>> {
    Ok(candidates(db)?.into_iter().find(|c| c.game.appid == appid))
}

pub fn name_of(db: &Db, appid: i64) -> String {
    db.with(|c| {
        Ok(c.query_row(
            "SELECT COALESCE(NULLIF(g.name,''), m.name, 'App ' || g.appid)
             FROM games g LEFT JOIN meta m ON m.appid=g.appid WHERE g.appid=?1",
            params![appid],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|_| format!("App {appid}")))
    })
    .unwrap_or_else(|_| format!("App {appid}"))
}
