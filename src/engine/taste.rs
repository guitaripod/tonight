use crate::db::{now, Db};
use crate::engine::load;
use crate::model::Candidate;
use anyhow::{anyhow, Result};
use rusqlite::params;
use serde::Serialize;

const K: f64 = 26.0;
const LR: f64 = 0.22;

fn dims(c: &Candidate) -> Vec<String> {
    let mut out: Vec<String> = c.meta.genres.iter().map(|g| format!("genre:{g}")).collect();
    out.extend(c.meta.tags.iter().take(8).map(|t| format!("tag:{t}")));
    out.extend(
        c.meta
            .categories
            .iter()
            .filter(|cat| {
                matches!(
                    cat.as_str(),
                    "Single-player"
                        | "Co-op"
                        | "Online Co-op"
                        | "Shared/Split Screen"
                        | "PvP"
                        | "Online PvP"
                )
            })
            .map(|cat| format!("cat:{cat}")),
    );
    out.push(format!("arch:{}", c.shape.archetype));
    out
}

fn apply(db: &Db, dims: &[String], signal: f64) -> Result<()> {
    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        for d in dims {
            tx.execute(
                "INSERT INTO taste(dim,weight,n) VALUES(?1,?2,1)
                 ON CONFLICT(dim) DO UPDATE SET
                   weight = MAX(-3.0, MIN(3.0, taste.weight + ?2)),
                   n = taste.n + 1",
                params![d, LR * signal],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
}

/// Moves the taste model for one game's dimensions. Positive means "more like this".
pub fn nudge(db: &Db, appid: i64, signal: f64) -> Result<()> {
    let Some(c) = load::one(db, appid)? else {
        return Ok(());
    };
    apply(db, &dims(&c), signal)
}

pub fn signal_for(verdict: &str) -> f64 {
    match verdict {
        "loved" => 1.0,
        "kept" => 0.6,
        "chosen" | "launched" => 0.4,
        "fine" => 0.1,
        "bounced" => -0.5,
        "never" => -1.4,
        "amnesty" => -0.9,
        "passed" => -0.15,
        _ => 0.0,
    }
}

#[derive(Debug, Serialize)]
pub struct DuelPair {
    pub a: DuelSide,
    pub b: DuelSide,
    pub round: i64,
    pub of: i64,
}

#[derive(Debug, Serialize)]
pub struct DuelSide {
    pub appid: i64,
    pub name: String,
    pub session_label: String,
    pub genres: Vec<String>,
    pub playtime_min: i64,
    pub short_desc: String,
}

fn side(c: &Candidate) -> DuelSide {
    DuelSide {
        appid: c.game.appid,
        name: c.game.name.clone(),
        session_label: c.shape.session_label.clone(),
        genres: c.meta.genres.clone(),
        playtime_min: c.game.playtime_min,
        short_desc: c.meta.short_desc.clone(),
    }
}

/// Picks two games whose Elo is close but whose archetypes differ, so each answer
/// carries real information instead of confirming the obvious.
pub fn duel(db: &Db, exclude: &[i64], round: i64, of: i64) -> Result<DuelPair> {
    let all = load::candidates(db)?;
    let pool: Vec<&Candidate> = load::eligible(&all)
        .into_iter()
        .filter(|c| !exclude.contains(&c.game.appid))
        .filter(|c| !c.meta.genres.is_empty())
        .collect();
    if pool.len() < 2 {
        return Err(anyhow!(
            "Not enough of your library is synced yet for duels."
        ));
    }

    let seed = now() as u64 ^ (round as u64).wrapping_mul(0x9E3779B9);
    let mut ranked: Vec<&&Candidate> = pool.iter().collect();
    ranked.sort_by_key(|c| {
        let uncertainty = 100 - c.prefs.duels.min(100);
        let jitter = ((seed ^ c.game.appid as u64).wrapping_mul(2654435761) % 40) as i64;
        -(uncertainty * 3 + jitter)
    });

    let a = ranked[0];
    let b = ranked
        .iter()
        .skip(1)
        .find(|c| {
            c.shape.archetype != a.shape.archetype && (c.prefs.elo - a.prefs.elo).abs() < 260.0
        })
        .or_else(|| ranked.get(1))
        .ok_or_else(|| anyhow!("no opponent"))?;

    Ok(DuelPair {
        a: side(a),
        b: side(b),
        round,
        of,
    })
}

pub fn duel_result(db: &Db, winner: i64, loser: i64) -> Result<()> {
    let (ra, rb) = db.with(|c| {
        let get = |id: i64| -> f64 {
            c.query_row("SELECT elo FROM prefs WHERE appid=?1", params![id], |r| {
                r.get::<_, f64>(0)
            })
            .unwrap_or(1500.0)
        };
        Ok((get(winner), get(loser)))
    })?;

    let expected = 1.0 / (1.0 + 10f64.powf((rb - ra) / 400.0));
    let new_a = ra + K * (1.0 - expected);
    let new_b = rb - K * (1.0 - expected);

    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        for (id, elo) in [(winner, new_a), (loser, new_b)] {
            tx.execute(
                "INSERT INTO prefs(appid,elo,duels,updated_at) VALUES(?1,?2,1,?3)
                 ON CONFLICT(appid) DO UPDATE SET elo=?2, duels=prefs.duels+1, updated_at=?3",
                params![id, elo, now()],
            )?;
        }
        tx.commit()?;
        Ok(())
    })?;

    db.log_event(winner, "duel_win", None, None, None, None)?;
    db.log_event(loser, "duel_loss", None, None, None, None)?;
    nudge(db, winner, 0.35)?;
    nudge(db, loser, -0.2)?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct TasteRow {
    pub dim: String,
    pub weight: f64,
    pub n: i64,
}

pub fn profile(db: &Db) -> Result<Vec<TasteRow>> {
    db.with(|c| {
        let mut st = c.prepare(
            "SELECT dim, weight, n FROM taste WHERE n >= 2 ORDER BY ABS(weight) DESC LIMIT 24",
        )?;
        let rows = st.query_map([], |r| {
            Ok(TasteRow {
                dim: r.get(0)?,
                weight: r.get(1)?,
                n: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
}
