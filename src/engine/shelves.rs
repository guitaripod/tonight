use crate::db::{now, Db};
use crate::engine::{load, score};
use crate::model::Candidate;
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ShelfDef {
    pub id: &'static str,
    pub title: &'static str,
    pub question: &'static str,
}

pub const SHELVES: &[ShelfDef] = &[
    ShelfDef {
        id: "installed",
        title: "Ready right now",
        question: "Already on the disk. No download, no excuse.",
    },
    ShelfDef {
        id: "sealed",
        title: "Still sealed",
        question: "Bought, filed, never once opened.",
    },
    ShelfDef {
        id: "abandoned",
        title: "You walked out early",
        question: "Under two hours in, then silence.",
    },
    ShelfDef {
        id: "nearly",
        title: "Nearly there",
        question: "One or two evenings from being finished.",
    },
    ShelfDef {
        id: "quick",
        title: "Under forty minutes",
        question: "For the nights where you only have a sliver.",
    },
    ShelfDef {
        id: "evening",
        title: "Clear the evening",
        question: "These want the whole night and will take it.",
    },
    ShelfDef {
        id: "together",
        title: "If someone's over",
        question: "Co-op, split-screen, or something to watch.",
    },
    ShelfDef {
        id: "ghosts",
        title: "Ghosts",
        question: "Three years or older, and you have never met them.",
    },
    ShelfDef {
        id: "loved",
        title: "You loved these once",
        question: "Old friends. They will still take you back.",
    },
    ShelfDef {
        id: "passed",
        title: "The ones you keep passing on",
        question: "The deck offered. You said no. Repeatedly.",
    },
    ShelfDef {
        id: "expensive",
        title: "Money on the shelf",
        question: "What the unopened ones cost at list price.",
    },
    ShelfDef {
        id: "retired",
        title: "Retired",
        question: "Granted amnesty. Out of the deck, off your conscience.",
    },
];

#[derive(Debug, Serialize)]
pub struct ShelfItem {
    pub appid: i64,
    pub name: String,
    pub detail: String,
    pub session_label: String,
    pub session_minutes: i64,
    pub playtime_min: i64,
    pub installed: bool,
    pub amnesty: bool,
    pub pinned: bool,
    pub genres: Vec<String>,
    pub metacritic: i64,
    pub release_year: i64,
    pub short_desc: String,
}

fn days_since(ts: i64) -> f64 {
    if ts <= 0 {
        return 9999.0;
    }
    ((now() - ts).max(0) as f64) / 86400.0
}

fn seen_fraction(c: &Candidate) -> f64 {
    if c.shape.completion_hours <= 0.0 {
        return 0.0;
    }
    (c.game.playtime_min as f64 / 60.0) / c.shape.completion_hours
}

fn item(c: &Candidate, detail: String) -> ShelfItem {
    ShelfItem {
        appid: c.game.appid,
        name: c.game.name.clone(),
        detail,
        session_label: c.shape.session_label.clone(),
        session_minutes: c.shape.session_minutes,
        playtime_min: c.game.playtime_min,
        installed: c.game.installed,
        amnesty: c.prefs.amnesty,
        pinned: c.prefs.pinned,
        genres: c.meta.genres.clone(),
        metacritic: c.meta.metacritic,
        release_year: c.meta.release_year,
        short_desc: c.meta.short_desc.clone(),
    }
}

pub fn shelf(db: &Db, id: &str, limit: usize) -> Result<Vec<ShelfItem>> {
    let all = load::candidates(db)?;
    let named: Vec<&Candidate> = all
        .iter()
        .filter(|c| !c.game.name.trim().is_empty())
        .collect();
    let live: Vec<&&Candidate> = named.iter().filter(|c| !c.prefs.amnesty).collect();
    let this_year = 1970 + (now() / 31_557_600);

    let mut out: Vec<ShelfItem> = match id {
        "installed" => {
            let mut v: Vec<&&Candidate> =
                live.iter().copied().filter(|c| c.game.installed).collect();
            v.sort_by_key(|c| (c.game.playtime_min, -c.meta.metacritic));
            v.into_iter()
                .map(|c| {
                    let d = if c.game.playtime_min == 0 {
                        "installed, never launched".to_string()
                    } else {
                        format!("{} played", score::minutes_phrase(c.game.playtime_min))
                    };
                    item(c, d)
                })
                .collect()
        }
        "sealed" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| c.game.playtime_min == 0)
                .collect();
            v.sort_by_key(|c| -c.meta.metacritic);
            v.into_iter()
                .map(|c| {
                    let d = if c.meta.metacritic > 0 {
                        format!("{} on Metacritic, 0 minutes played", c.meta.metacritic)
                    } else {
                        "0 minutes played".into()
                    };
                    item(c, d)
                })
                .collect()
        }
        "abandoned" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| {
                    (10..=120).contains(&c.game.playtime_min)
                        && days_since(c.game.last_played) > 45.0
                })
                .collect();
            v.sort_by_key(|c| -(c.game.playtime_min));
            v.into_iter()
                .map(|c| {
                    let d = format!(
                        "{} in, then stopped — {}",
                        score::minutes_phrase(c.game.playtime_min),
                        score::ago(days_since(c.game.last_played))
                    );
                    item(c, d)
                })
                .collect()
        }
        "nearly" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| {
                    let f = seen_fraction(c);
                    (0.55..0.98).contains(&f)
                        || (c.history.ach_total > 0
                            && (0.6..0.98).contains(
                                &(c.history.ach_unlocked as f64 / c.history.ach_total as f64),
                            ))
                })
                .collect();
            v.sort_by(|a, b| seen_fraction(b).partial_cmp(&seen_fraction(a)).unwrap());
            v.into_iter()
                .map(|c| {
                    let left =
                        (c.shape.completion_hours - c.game.playtime_min as f64 / 60.0).max(0.5);
                    let d = if c.history.ach_total > 0 && c.history.ach_unlocked > 0 {
                        format!(
                            "{}/{} achievements — roughly {:.0}h left",
                            c.history.ach_unlocked, c.history.ach_total, left
                        )
                    } else {
                        format!("roughly {:.0} hours from done", left)
                    };
                    item(c, d)
                })
                .collect()
        }
        "quick" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| c.shape.session_minutes <= 40 && c.shape.confidence > 0.3)
                .collect();
            v.sort_by_key(|c| c.shape.session_minutes);
            v.into_iter()
                .map(|c| {
                    let d = format!(
                        "{} — {}",
                        score::capitalise(&c.shape.session_label),
                        c.shape.archetype
                    );
                    item(c, d)
                })
                .collect()
        }
        "evening" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| c.shape.session_minutes >= 100)
                .collect();
            v.sort_by_key(|c| -c.shape.session_minutes);
            v.into_iter()
                .map(|c| {
                    let d = format!(
                        "{} a sitting, about {:.0}h to see it through",
                        c.shape.session_minutes, c.shape.completion_hours
                    );
                    item(c, d)
                })
                .collect()
        }
        "together" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| c.shape.social > 0.5)
                .collect();
            v.sort_by_key(|c| -c.meta.metacritic);
            v.into_iter()
                .map(|c| {
                    let modes: Vec<&str> = c
                        .meta
                        .categories
                        .iter()
                        .filter(|x| x.contains("Co-op") || x.contains("Split") || x.contains("PvP"))
                        .map(|x| x.as_str())
                        .take(2)
                        .collect();
                    item(
                        c,
                        if modes.is_empty() {
                            "multiplayer".into()
                        } else {
                            modes.join(" · ")
                        },
                    )
                })
                .collect()
        }
        "ghosts" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| {
                    c.game.playtime_min == 0
                        && c.meta.release_year > 1990
                        && this_year - c.meta.release_year >= 3
                })
                .collect();
            v.sort_by_key(|c| c.meta.release_year);
            v.into_iter()
                .map(|c| {
                    let d = format!(
                        "{} — {} years unopened",
                        c.meta.release_year,
                        this_year - c.meta.release_year
                    );
                    item(c, d)
                })
                .collect()
        }
        "loved" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| c.game.playtime_min >= 300 || c.history.loved > 0)
                .collect();
            v.sort_by_key(|c| -c.game.playtime_min);
            v.into_iter()
                .map(|c| {
                    let d = format!(
                        "{} · last opened {}",
                        score::minutes_phrase(c.game.playtime_min),
                        score::ago(days_since(c.game.last_played))
                    );
                    item(c, d)
                })
                .collect()
        }
        "passed" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| c.history.passes >= 2)
                .collect();
            v.sort_by_key(|c| -c.history.passes);
            v.into_iter()
                .map(|c| {
                    let d = format!("passed on {} times — retire it?", c.history.passes);
                    item(c, d)
                })
                .collect()
        }
        "expensive" => {
            let mut v: Vec<&&Candidate> = live
                .iter()
                .copied()
                .filter(|c| c.game.playtime_min == 0 && c.meta.price_initial > 0)
                .collect();
            v.sort_by_key(|c| -c.meta.price_initial);
            v.into_iter()
                .map(|c| {
                    let d = format!(
                        "{:.2} {} on the shelf, unopened",
                        c.meta.price_initial as f64 / 100.0,
                        c.meta.currency
                    );
                    item(c, d)
                })
                .collect()
        }
        "retired" => {
            let mut v: Vec<&&Candidate> = named.iter().filter(|c| c.prefs.amnesty).collect();
            v.sort_by(|a, b| a.game.name.cmp(&b.game.name));
            v.into_iter()
                .map(|c| item(c, "retired — out of the deck".into()))
                .collect()
        }
        _ => vec![],
    };
    out.truncate(limit);
    Ok(out)
}

/// Deliberately search-first: there is no browsable wall of boxart anywhere in
/// this app, because the wall is what caused the paralysis.
pub fn search(db: &Db, q: &str, limit: usize) -> Result<Vec<ShelfItem>> {
    let needle = q.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(vec![]);
    }
    let all = load::candidates(db)?;
    let mut hits: Vec<(&Candidate, i64)> = all
        .iter()
        .filter(|c| !c.game.name.trim().is_empty())
        .filter_map(|c| {
            let name = c.game.name.to_ascii_lowercase();
            let rank = if name == needle {
                0
            } else if name.starts_with(&needle) {
                1
            } else if name.split_whitespace().any(|w| w.starts_with(&needle)) {
                2
            } else if name.contains(&needle) {
                3
            } else if c
                .meta
                .genres
                .iter()
                .any(|g| g.to_ascii_lowercase().contains(&needle))
                || c.shape.archetype.contains(&needle)
            {
                4
            } else if c.meta.short_desc.to_ascii_lowercase().contains(&needle) {
                5
            } else {
                return None;
            };
            Some((c, rank))
        })
        .collect();
    hits.sort_by_key(|(c, rank)| (*rank, -c.game.playtime_min, c.game.name.clone()));
    Ok(hits
        .into_iter()
        .take(limit)
        .map(|(c, _)| {
            let d = if c.game.playtime_min == 0 {
                "never played".to_string()
            } else {
                format!(
                    "{} · {}",
                    score::minutes_phrase(c.game.playtime_min),
                    score::ago(days_since(c.game.last_played))
                )
            };
            item(c, d)
        })
        .collect())
}
