use crate::db::{now, Db};
use crate::engine::conditions::{self, ConditionDef};
use crate::engine::{load, score};
use crate::model::{Candidate, Card, DealRequest, Hand, Slot};
use anyhow::{anyhow, Result};
use rusqlite::params;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// How strict the deal is, loosest last. Each rung is (condition floor, time
/// floor). The dealer starts at the strictest and only steps down when too few
/// games survive, and says so when it does.
const LADDER: [(f64, f64); 4] = [(0.42, 0.30), (0.32, 0.22), (0.22, 0.12), (0.12, 0.05)];

/// Fewer survivors than this and the dealer loosens a rung.
const MIN_POOL: usize = 8;

/// Fewer survivors than this and the dealer stops loosening and says what is left.
const MIN_HAND: usize = 3;

pub struct Deck<'a> {
    pub scored: Vec<(&'a Candidate, score::Scored)>,
    pub library: usize,
    pub relaxed: Option<String>,
    pub blame: Option<String>,
}

fn seed_from(req: &DealRequest, salt: u64) -> u64 {
    let mut s = now() as u64 / 30;
    s = s.wrapping_mul(0x9E3779B97F4A7C15) ^ (req.budget as u64).wrapping_mul(31);
    for b in req
        .mood
        .bytes()
        .chain(req.appetite.bytes())
        .chain(req.conditions.iter().flat_map(|c| c.bytes()))
    {
        s = s.rotate_left(7) ^ b as u64;
    }
    s ^ salt
}

fn primary_genre(c: &Candidate) -> String {
    c.meta
        .genres
        .iter()
        .find(|g| {
            !matches!(
                g.to_ascii_lowercase().as_str(),
                "indie" | "early access" | "free to play"
            )
        })
        .or_else(|| c.meta.genres.first())
        .cloned()
        .unwrap_or_else(|| c.shape.archetype.clone())
}

fn seen_fraction(c: &Candidate) -> f64 {
    if c.shape.completion_hours <= 0.0 {
        return 0.0;
    }
    (c.game.playtime_min as f64 / 60.0) / c.shape.completion_hours
}

fn days_since(ts: i64) -> f64 {
    if ts <= 0 {
        return 9999.0;
    }
    ((now() - ts).max(0) as f64) / 86400.0
}

fn in_slot(c: &Candidate, slot: Slot) -> bool {
    match slot {
        Slot::SureThing => c.game.playtime_min >= 90 || c.history.loved > 0,
        Slot::Unfinished => c.game.playtime_min >= 10 && seen_fraction(c) < 0.7,
        Slot::Stranger => c.game.playtime_min <= 10,
        _ => true,
    }
}

/// The middle slot is one seat with two faces: a game you walked away from long
/// ago owes you a fair trial, one you left last week is simply waiting.
fn face_of(c: &Candidate, slot: Slot) -> Slot {
    if slot == Slot::Unfinished && days_since(c.game.last_played) < 21.0 {
        Slot::Thread
    } else {
        slot
    }
}

/// Fills in the defaults a request leaves out and folds the retired "social"
/// mood into the friends condition.
pub fn normalize(req: &DealRequest) -> DealRequest {
    let mut req = req.clone();
    if req.mood.is_empty() {
        req.mood = "mellow".into();
    }
    if req.appetite.is_empty() {
        req.appetite = "surprise".into();
    }
    if req.mood == "social" && !req.conditions.iter().any(|c| c == "friends") {
        req.conditions.push("friends".into());
    }
    req.conditions = conditions::normalize(&req.conditions)
        .iter()
        .map(|c| c.id.to_string())
        .collect();
    req
}

fn names_of(defs: &[&ConditionDef]) -> String {
    let labels: Vec<String> = defs.iter().map(|d| format!("“{}”", d.label)).collect();
    match labels.len() {
        0 => String::new(),
        1 => labels[0].clone(),
        _ => format!(
            "{} and {}",
            labels[..labels.len() - 1].join(", "),
            labels[labels.len() - 1]
        ),
    }
}

/// Builds the scored deck for a given context. Circumstances come first: they
/// decide who may be dealt at all, loosening only when the library is too small
/// to honour them, and saying so. Only then does taste order the survivors.
pub fn deck<'a>(
    all: &'a [Candidate],
    req: &DealRequest,
    seed: u64,
    taste: &HashMap<String, f64>,
) -> Deck<'a> {
    let conds = conditions::normalize(&req.conditions);
    let ctx = score::Context {
        budget: req.budget,
        mood: req.mood.clone(),
        appetite: req.appetite.clone(),
        conditions: conds.clone(),
        taste: taste.clone(),
        seed,
    };
    let excluded: HashSet<i64> = req.exclude.iter().copied().collect();
    let pool: Vec<&Candidate> = load::eligible(all)
        .into_iter()
        .filter(|c| !excluded.contains(&c.game.appid))
        .collect();
    let library = pool.len();

    let scored: Vec<(&Candidate, score::Scored)> = pool
        .into_iter()
        .map(|c| (c, score::score(c, &ctx)))
        .collect();

    let passes = |s: &score::Scored, cond_floor: f64, time_floor: f64| -> bool {
        (req.budget <= 0 || s.fit >= time_floor) && s.fits.iter().all(|f| f.clears(cond_floor))
    };
    let survivors = |rung: usize| -> Vec<usize> {
        let (cond_floor, time_floor) = LADDER[rung];
        scored
            .iter()
            .enumerate()
            .filter(|(_, (_, s))| passes(s, cond_floor, time_floor))
            .map(|(i, _)| i)
            .collect()
    };

    let strict_len = survivors(0).len();
    let mut kept = survivors(0);
    let mut rung = 0;
    while kept.len() < MIN_POOL && rung + 1 < LADDER.len() {
        rung += 1;
        kept = survivors(rung);
    }

    let mut relaxed = None;
    if kept.len() > strict_len {
        relaxed = Some(loosened_note(&scored, &conds, req.budget, &passes));
    }

    if kept.len() < MIN_HAND {
        let loosest = LADDER[LADDER.len() - 1].0;
        let ignoring_time: Vec<usize> = scored
            .iter()
            .enumerate()
            .filter(|(_, (_, s))| passes(s, loosest, 0.0))
            .map(|(i, _)| i)
            .collect();
        if req.budget > 0 && ignoring_time.len() > kept.len() {
            kept = ignoring_time;
            relaxed = Some(format!(
                "Nothing fits {} cleanly, so these are the closest.",
                score::humanize_budget(req.budget)
            ));
        } else if !kept.is_empty() && conds.iter().any(|d| d.id != "installed") {
            relaxed = Some(format!(
                "Only {} of your games work with {}.",
                kept.len(),
                names_of(&conds)
            ));
        }
    }

    let blame = kept
        .is_empty()
        .then(|| blame_message(&scored, &conds, library));

    let mut chosen: Vec<(&Candidate, score::Scored)> =
        kept.into_iter().map(|i| scored[i].clone()).collect();
    chosen.sort_by(|a, b| {
        b.1.total
            .partial_cmp(&a.1.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Deck {
        scored: chosen,
        library,
        relaxed,
        blame,
    }
}

/// Which conditions cost the most games at the strictest rung, in words.
fn loosened_note(
    scored: &[(&Candidate, score::Scored)],
    conds: &[&'static ConditionDef],
    budget: i64,
    passes: &dyn Fn(&score::Scored, f64, f64) -> bool,
) -> String {
    let (cond_floor, time_floor) = LADDER[0];
    let mut cuts: Vec<(&'static ConditionDef, usize)> = conds
        .iter()
        .filter(|d| d.id != "installed")
        .map(|d| {
            let failing = scored
                .iter()
                .filter(|(_, s)| s.fits.iter().any(|f| f.id == d.id && !f.clears(cond_floor)))
                .count();
            (*d, failing)
        })
        .filter(|(_, n)| *n > 0)
        .collect();
    cuts.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let tight: Vec<&ConditionDef> = cuts.iter().take(2).map(|(d, _)| *d).collect();
    let time_cuts = budget > 0
        && scored
            .iter()
            .filter(|(_, s)| !passes(s, 0.0, time_floor))
            .count()
            > scored.len() / 3;
    let time = score::humanize_budget(budget);
    match (tight.is_empty(), time_cuts) {
        (false, true) => format!(
            "Few games fit {time} and meet {} strictly, so I loosened both a little.",
            names_of(&tight)
        ),
        (false, false) => format!(
            "Few games meet {} strictly, so I loosened it a little.",
            names_of(&tight)
        ),
        (true, _) => format!("Few games fit {time} strictly, so I loosened the time a little."),
    }
}

/// When nothing survives, the most useful sentence names the culprit.
fn blame_message(
    scored: &[(&Candidate, score::Scored)],
    conds: &[&'static ConditionDef],
    library: usize,
) -> String {
    if library == 0 {
        return "No games left to deal. Sync your library, or lift a few amnesties.".into();
    }
    let floor = LADDER[LADDER.len() - 1].0;
    let mut worst: Option<(&ConditionDef, usize)> = None;
    for d in conds {
        let passing = scored
            .iter()
            .filter(|(_, s)| s.fits.iter().any(|f| f.id == d.id && f.clears(floor)))
            .count();
        if worst.map(|(_, n)| passing < n).unwrap_or(true) {
            worst = Some((d, passing));
        }
    }
    match worst {
        Some((d, n)) if conds.len() == 1 => format!(
            "Only {n} of your {library} games pass “{}”. Untick it to widen the net.",
            d.label
        ),
        Some((d, n)) => format!(
            "No game passes all of {} together. “{}” is the tightest, at {n} of {library}.",
            names_of(conds),
            d.label
        ),
        None => "Nothing in the library fits tonight. Try a longer evening.".into(),
    }
}

fn similarity(a: &Candidate, b: &Candidate) -> f64 {
    let axes = |c: &Candidate| {
        [
            c.shape.pace,
            c.shape.brain,
            c.shape.story,
            c.shape.social,
            c.shape.chill,
        ]
    };
    let (x, y) = (axes(a), axes(b));
    let feel = 1.0 - x.iter().zip(y).map(|(p, q)| (p - q).abs()).sum::<f64>() / 5.0;
    let mut sim = feel * 0.55;
    if a.shape.archetype == b.shape.archetype {
        sim += 0.3;
    }
    if primary_genre(a) == primary_genre(b) {
        sim += 0.15;
    }
    sim.clamp(0.0, 1.0)
}

const ASSEMBLY_REPEL: f64 = 0.7;
const SHORTLIST: usize = 5;
const WARMTH: f64 = 0.22;

/// Chooses among the best few for a slot instead of always taking the top one:
/// near-equals get a fair draw, so dealing again feels like a new hand rather
/// than the same one shuffled, and a clear winner still almost always wins.
fn draw(shortlist: &[(usize, f64)], roll: f64) -> usize {
    let best = shortlist[0].1;
    let weights: Vec<f64> = shortlist
        .iter()
        .map(|(_, adj)| ((adj - best) / WARMTH).exp())
        .collect();
    let mut point = roll * weights.iter().sum::<f64>();
    for (k, w) in weights.iter().enumerate() {
        if point < *w {
            return shortlist[k].0;
        }
        point -= w;
    }
    shortlist[shortlist.len() - 1].0
}

/// Picks one card per kind of evening rather than the global top three, so the
/// player compares kinds of evening instead of comparing 400 interchangeable
/// games. Each pick is pushed away from what is already on the table.
fn pick_triad(deck: &Deck, appetite: &str, seed: u64) -> Vec<(Slot, usize)> {
    let mut order = vec![Slot::SureThing, Slot::Unfinished, Slot::Stranger];
    let preferred = match appetite {
        "comfort" => Some(Slot::SureThing),
        "unfinished" => Some(Slot::Unfinished),
        "stranger" => Some(Slot::Stranger),
        _ => None,
    };
    if let Some(p) = preferred {
        order.retain(|s| *s != p);
        order.insert(0, p);
    }

    let mut taken: Vec<usize> = vec![];
    let mut out: Vec<(Slot, usize)> = vec![];

    let adjusted = |i: usize, taken: &[usize]| -> f64 {
        let repel = taken
            .iter()
            .map(|t| similarity(deck.scored[i].0, deck.scored[*t].0))
            .fold(0.0, f64::max);
        deck.scored[i].1.total - ASSEMBLY_REPEL * repel
    };

    for (n, slot) in order.into_iter().enumerate() {
        let mut shortlist: Vec<(usize, f64)> = deck
            .scored
            .iter()
            .enumerate()
            .filter(|(i, (c, _))| !taken.contains(i) && in_slot(c, slot))
            .map(|(i, _)| (i, adjusted(i, &taken)))
            .collect();
        if shortlist.is_empty() {
            continue;
        }
        shortlist.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        shortlist.truncate(SHORTLIST);
        let roll = score::splitmix(seed ^ (n as u64 + 1).wrapping_mul(0xA24BAED4963EE407));
        let pick = draw(&shortlist, roll);
        taken.push(pick);
        out.push((slot, pick));
    }

    while out.len() < 3 {
        let best = deck
            .scored
            .iter()
            .enumerate()
            .filter(|(i, _)| !taken.contains(i))
            .map(|(i, _)| (i, adjusted(i, &taken)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        match best {
            Some((i, _)) => {
                taken.push(i);
                out.push((Slot::Wildcard, i));
            }
            None => break,
        }
    }
    out
}

/// A card's headline must agree with the role it was dealt into. A comfort-food
/// argument under "Unfinished business" reads as a bug, even when the score is right.
fn suits_slot(kind: &str, slot: Slot) -> bool {
    match kind {
        "appetite:comfort" => slot == Slot::SureThing || slot == Slot::Wildcard,
        "appetite:unfinished" => {
            matches!(slot, Slot::Unfinished | Slot::Thread | Slot::Wildcard)
        }
        "appetite:stranger" => slot == Slot::Stranger || slot == Slot::Wildcard,
        _ => true,
    }
}

fn to_card(c: &Candidate, s: &score::Scored, slot: Slot) -> Card {
    let slot = face_of(c, slot);
    let verdict = headline(c, s, slot);
    Card {
        appid: c.game.appid,
        name: c.game.name.clone(),
        slot,
        slot_label: slot.label().into(),
        slot_blurb: slot.blurb(c.game.playtime_min).into(),
        score: (s.total * 1000.0).round() / 1000.0,
        verdict,
        caution: s.caution.clone(),
        reasons: s.reasons.clone(),
        fits: s.fits.clone(),
        parts: s.parts.clone(),
        gate: (s.gate * 1000.0).round() / 1000.0,
        session_label: c.shape.session_label.clone(),
        session_minutes: c.shape.session_minutes,
        playtime_min: c.game.playtime_min,
        last_played: c.game.last_played,
        installed: c.game.installed,
        genres: c.meta.genres.clone(),
        tags: c.meta.tags.iter().take(5).cloned().collect(),
        metacritic: c.meta.metacritic,
        review_pct: c.meta.review_pct,
        short_desc: c.meta.short_desc.clone(),
        achievements_total: c.history.ach_total,
        release_year: c.meta.release_year,
    }
}

/// The sentence under the title. The argument for the role the card was dealt
/// into comes first; when the role has none to make, the strongest reason that
/// does not belong to another role stands in.
fn headline(c: &Candidate, s: &score::Scored, slot: Slot) -> String {
    let appetite = s
        .reasons
        .iter()
        .find(|r| r.kind.starts_with("appetite:") && suits_slot(&r.kind, slot));
    if let Some(r) = appetite {
        return r.text.clone();
    }
    if matches!(
        slot,
        Slot::SureThing | Slot::Unfinished | Slot::Thread | Slot::Stranger
    ) {
        return default_verdict(c, slot);
    }
    s.reasons
        .iter()
        .find(|r| suits_slot(&r.kind, slot))
        .map(|r| r.text.clone())
        .unwrap_or_else(|| default_verdict(c, slot))
}

fn default_verdict(c: &Candidate, slot: Slot) -> String {
    match slot {
        Slot::Stranger => {
            if c.game.playtime_min == 0 {
                "You bought it. You have never once opened it.".into()
            } else {
                format!(
                    "{}, once. That is a rounding error, not an opinion.",
                    score::minutes_phrase(c.game.playtime_min)
                )
            }
        }
        Slot::SureThing => format!(
            "{} on the clock already — this one never disappoints you.",
            score::minutes_phrase(c.game.playtime_min)
        ),
        Slot::Unfinished => format!(
            "Last touched {}. It is still waiting.",
            score::ago(days_since(c.game.last_played))
        ),
        Slot::Thread => format!(
            "{} in, last touched {}. Pick it back up.",
            score::minutes_phrase(c.game.playtime_min),
            score::ago(days_since(c.game.last_played))
        ),
        _ => format!(
            "{}. No further argument.",
            score::capitalise(&c.shape.session_label)
        ),
    }
}

pub fn hand_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub viable: usize,
    pub library: usize,
    pub relaxed: Option<String>,
    pub blame: Option<String>,
}

/// How many games would survive tonight's conditions, without dealing anything.
/// The intake screen calls this on every tap so an impossible evening is visible
/// before the deal rather than after it.
pub fn preview(db: &Db, req: &DealRequest) -> Result<Preview> {
    let all = load::candidates(db)?;
    let taste = load::taste_map(db)?;
    let req = normalize(req);
    let deck = deck(&all, &req, 0, &taste);
    Ok(Preview {
        viable: deck.scored.len(),
        library: deck.library,
        relaxed: deck.relaxed,
        blame: deck.blame,
    })
}

pub fn deal(db: &Db, req: &DealRequest) -> Result<Hand> {
    let all = load::candidates(db)?;
    let taste = load::taste_map(db)?;
    let req = normalize(req);

    let rerolls = match &req.reroll_of {
        Some(prev) => previous_rerolls(db, prev)? + 1,
        None => 0,
    };
    let seed = seed_from(&req, rerolls.wrapping_mul(7919) as u64);
    let deck = deck(&all, &req, seed, &taste);
    if deck.scored.is_empty() {
        return Err(anyhow!(deck
            .blame
            .clone()
            .unwrap_or_else(|| "No games left to deal.".into())));
    }

    let picks = pick_triad(&deck, &req.appetite, seed);
    let cards: Vec<Card> = picks
        .iter()
        .map(|(slot, i)| {
            let (c, s) = &deck.scored[*i];
            to_card(c, s, *slot)
        })
        .collect();

    let id = hand_id();
    let json = serde_json::to_string(&cards)?;
    let conditions_json = serde_json::to_string(&req.conditions)?;
    let viable = deck.scored.len();
    db.with(|c| {
        c.execute(
            "DELETE FROM hands WHERE ts < ?1",
            params![now() - 30 * 86400],
        )?;
        c.execute(
            "INSERT INTO hands(id,ts,budget,mood,appetite,cards,passes,rerolls,reroll_of,
                               conditions,viable,library,relaxed)
             VALUES(?1,?2,?3,?4,?5,?6,0,?7,?8,?9,?10,?11,?12)",
            params![
                id,
                now(),
                req.budget,
                req.mood,
                req.appetite,
                json,
                rerolls,
                req.reroll_of,
                conditions_json,
                viable as i64,
                deck.library as i64,
                deck.relaxed
            ],
        )?;
        Ok(())
    })?;
    for card in &cards {
        db.log_event(
            card.appid,
            "dealt",
            Some(req.budget),
            Some(&req.mood),
            Some(&req.appetite),
            None,
        )?;
    }

    Ok(Hand {
        id,
        passes_allowed: (cards.len() as i64 - 1).max(0),
        locked: cards.len() <= 1,
        cards,
        passes_used: 0,
        rerolls,
        budget: req.budget,
        mood: req.mood,
        appetite: req.appetite,
        note: reroll_note(rerolls),
        conditions: req.conditions,
        viable,
        library: deck.library,
        relaxed: deck.relaxed,
    })
}

fn reroll_note(rerolls: i64) -> Option<String> {
    match rerolls {
        0 => None,
        1 => Some("Second deal. The deck remembers what you turned down.".into()),
        2 => Some(
            "Third deal. You are browsing again — this is the thing you said you hated.".into(),
        ),
        n => Some(format!(
            "Deal number {}. Every game you rejected is gone from the deck. Pick one.",
            n + 1
        )),
    }
}

fn previous_rerolls(db: &Db, hand: &str) -> Result<i64> {
    db.with(|c| {
        Ok(c.query_row(
            "SELECT rerolls FROM hands WHERE id=?1",
            params![hand],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0))
    })
}

struct HandRow {
    budget: i64,
    mood: String,
    appetite: String,
    cards: String,
    passes: i64,
    rerolls: i64,
    conditions: String,
    viable: i64,
    library: i64,
    relaxed: Option<String>,
}

pub fn load_hand(db: &Db, id: &str) -> Result<Hand> {
    db.with(|c| {
        let row = c.query_row(
            "SELECT budget, mood, appetite, cards, passes, rerolls, conditions, viable, library,
                    relaxed
             FROM hands WHERE id=?1",
            params![id],
            |r| {
                Ok(HandRow {
                    budget: r.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    mood: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    appetite: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    cards: r.get(3)?,
                    passes: r.get(4)?,
                    rerolls: r.get(5)?,
                    conditions: r.get(6)?,
                    viable: r.get(7)?,
                    library: r.get(8)?,
                    relaxed: r.get(9)?,
                })
            },
        )?;
        let cards: Vec<Card> = serde_json::from_str(&row.cards)?;
        Ok(Hand {
            id: id.to_string(),
            passes_allowed: 2,
            locked: cards.len() <= 1,
            cards,
            passes_used: row.passes,
            rerolls: row.rerolls,
            budget: row.budget,
            mood: row.mood,
            appetite: row.appetite,
            note: reroll_note(row.rerolls),
            conditions: serde_json::from_str(&row.conditions).unwrap_or_default(),
            viable: row.viable.max(0) as usize,
            library: row.library.max(0) as usize,
            relaxed: row.relaxed,
        })
    })
}

/// Removes a card from a live hand. The last card standing is not removable —
/// that is the entire point of the exercise.
pub fn pass(db: &Db, id: &str, appid: i64) -> Result<Hand> {
    let mut hand = load_hand(db, id)?;
    if hand.cards.len() <= 1 || !hand.cards.iter().any(|c| c.appid == appid) {
        return Ok(hand);
    }
    hand.cards.retain(|c| c.appid != appid);
    hand.passes_used += 1;
    hand.locked = hand.cards.len() <= 1;
    let json = serde_json::to_string(&hand.cards)?;
    db.with(|c| {
        c.execute(
            "UPDATE hands SET cards=?2, passes=?3 WHERE id=?1",
            params![id, json, hand.passes_used],
        )?;
        Ok(())
    })?;
    db.log_event(
        appid,
        "passed",
        Some(hand.budget),
        Some(&hand.mood),
        Some(&hand.appetite),
        None,
    )?;
    crate::engine::taste::nudge(db, appid, -0.15)?;
    if hand.locked {
        hand.note = Some("That's the one. No more passes.".into());
    }
    Ok(hand)
}

/// A single replacement card after a bail-out, excluding everything already seen.
pub fn rebound(db: &Db, req: &DealRequest) -> Result<Card> {
    let all = load::candidates(db)?;
    let taste = load::taste_map(db)?;
    let req = normalize(req);
    let seed = seed_from(&req, 0xBEEF);
    let deck = deck(&all, &req, seed, &taste);
    let (c, s) = deck.scored.first().ok_or_else(|| {
        anyhow!(deck
            .blame
            .clone()
            .unwrap_or_else(|| "nothing left to suggest".into()))
    })?;
    Ok(to_card(c, s, Slot::Rebound))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::shape::{self, Personal};
    use crate::model::{Game, History, Meta, Prefs};

    fn candidate(
        appid: i64,
        name: &str,
        tags: &[&str],
        categories: &[&str],
        mins: i64,
    ) -> Candidate {
        let game = Game {
            appid,
            name: name.into(),
            playtime_min: mins,
            playtime_2w: 0,
            last_played: if mins > 0 { now() - 90 * 86400 } else { 0 },
            installed: true,
            size_bytes: 0,
        };
        let meta = Meta {
            kind: "game".into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            categories: categories.iter().map(|s| s.to_string()).collect(),
            genres: vec!["Strategy".into()],
            ..Default::default()
        };
        let shape = shape::derive(&game, &meta, &Personal::default());
        Candidate {
            game,
            meta,
            prefs: Prefs::default(),
            shape,
            history: History::default(),
        }
    }

    fn library() -> Vec<Candidate> {
        let quiet_tags: [&[&str]; 4] = [
            &["Turn-Based Strategy", "Strategy", "Singleplayer"],
            &["Puzzle", "Singleplayer", "Relaxing"],
            &["City Builder", "Management", "Singleplayer"],
            &["Idler", "Casual", "Singleplayer"],
        ];
        let loud_tags: [&[&str]; 4] = [
            &["Horror", "Atmospheric", "Singleplayer"],
            &["Rhythm", "Music", "Singleplayer"],
            &["FPS", "Multiplayer", "PvP", "Competitive"],
            &["Stealth", "Atmospheric", "Singleplayer"],
        ];
        let mut out = vec![];
        let mut id = 1;
        for round in 0..4 {
            for tags in quiet_tags {
                out.push(candidate(
                    id,
                    &format!("Quiet {id}"),
                    tags,
                    &["Single-player", "Save Anytime"],
                    if round == 0 { 0 } else { 200 * round },
                ));
                id += 1;
            }
            for tags in loud_tags {
                out.push(candidate(
                    id,
                    &format!("Loud {id}"),
                    tags,
                    &["Single-player"],
                    if round == 0 { 0 } else { 200 * round },
                ));
                id += 1;
            }
        }
        out
    }

    fn request(conditions: &[&str]) -> DealRequest {
        normalize(&DealRequest {
            budget: 0,
            mood: "mellow".into(),
            appetite: "surprise".into(),
            conditions: conditions.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
    }

    #[test]
    fn sound_taken_never_deals_a_game_that_needs_sound() {
        let all = library();
        let req = request(&["quiet"]);
        for seed in 0..40u64 {
            let d = deck(&all, &req, seed, &HashMap::new());
            let picks = pick_triad(&d, &req.appetite, seed);
            assert!(!picks.is_empty());
            for (_, i) in picks {
                let name = &d.scored[i].0.game.name;
                assert!(
                    name.starts_with("Quiet"),
                    "dealt {name} with the sound taken"
                );
            }
        }
    }

    #[test]
    fn without_conditions_the_whole_library_is_in_play() {
        let all = library();
        let req = request(&[]);
        let d = deck(&all, &req, 1, &HashMap::new());
        assert_eq!(d.scored.len(), all.len());
        assert!(d.relaxed.is_none());
    }

    #[test]
    fn the_dealer_loosens_and_says_so_when_the_library_is_thin() {
        let mut all = library();
        all.retain(|c| c.game.name.starts_with("Loud") || c.game.appid <= 2);
        let req = request(&["quiet"]);
        let d = deck(&all, &req, 1, &HashMap::new());
        assert!(d.scored.len() <= all.len());
        assert!(
            d.relaxed.is_some() || d.scored.len() >= MIN_POOL,
            "a thin library must produce a note"
        );
    }

    #[test]
    fn an_impossible_evening_names_the_culprit() {
        let mut all = library();
        for c in &mut all {
            c.game.installed = false;
        }
        let req = request(&["installed", "quiet"]);
        let d = deck(&all, &req, 1, &HashMap::new());
        assert!(d.scored.is_empty());
        let blame = d.blame.expect("a reason");
        assert!(blame.contains("No downloads"), "{blame}");
    }

    #[test]
    fn dealing_is_deterministic_for_a_seed_and_varies_across_seeds() {
        let all = library();
        let req = request(&[]);
        let hands: Vec<Vec<i64>> = (0..30u64)
            .map(|seed| {
                let d = deck(&all, &req, seed, &HashMap::new());
                pick_triad(&d, &req.appetite, seed)
                    .iter()
                    .map(|(_, i)| d.scored[*i].0.game.appid)
                    .collect()
            })
            .collect();
        let again: Vec<i64> = {
            let d = deck(&all, &req, 7, &HashMap::new());
            pick_triad(&d, &req.appetite, 7)
                .iter()
                .map(|(_, i)| d.scored[*i].0.game.appid)
                .collect()
        };
        assert_eq!(hands[7], again);
        let distinct: HashSet<&Vec<i64>> = hands.iter().collect();
        assert!(
            distinct.len() > 3,
            "only {} distinct hands in 30 deals",
            distinct.len()
        );
    }

    #[test]
    fn a_hand_has_no_repeats() {
        let all = library();
        let req = request(&[]);
        for seed in 0..25u64 {
            let d = deck(&all, &req, seed, &HashMap::new());
            let ids: Vec<usize> = pick_triad(&d, &req.appetite, seed)
                .iter()
                .map(|(_, i)| *i)
                .collect();
            let unique: HashSet<&usize> = ids.iter().collect();
            assert_eq!(ids.len(), unique.len());
            assert_eq!(ids.len(), 3);
        }
    }

    #[test]
    fn the_social_mood_becomes_the_friends_condition() {
        let req = normalize(&DealRequest {
            mood: "social".into(),
            ..Default::default()
        });
        assert!(req.conditions.contains(&"friends".to_string()));
    }

    #[test]
    fn a_recent_game_is_dealt_as_the_thread_not_as_abandoned() {
        let mut c = candidate(1, "Fresh", &["Strategy"], &[], 300);
        c.game.last_played = now() - 3 * 86400;
        assert_eq!(face_of(&c, Slot::Unfinished), Slot::Thread);
        c.game.last_played = now() - 90 * 86400;
        assert_eq!(face_of(&c, Slot::Unfinished), Slot::Unfinished);
    }

    #[test]
    fn nothing_is_announced_when_nothing_was_loosened() {
        let mut all = library();
        for (n, c) in all.iter_mut().enumerate() {
            c.game.installed = n < 5;
        }
        let req = request(&["installed"]);
        let d = deck(&all, &req, 1, &HashMap::new());
        assert_eq!(d.scored.len(), 5);
        assert!(d.relaxed.is_none(), "{:?}", d.relaxed);
    }

    #[test]
    fn an_impossible_budget_falls_back_and_says_so_even_with_a_soft_condition() {
        let tags = ["Grand Strategy", "Turn-Based Strategy", "Singleplayer"];
        let all: Vec<Candidate> = (1..=12)
            .map(|i| {
                candidate(
                    i,
                    &format!("Epic {i}"),
                    &tags,
                    &["Single-player", "Save Anytime"],
                    300,
                )
            })
            .collect();
        let mut req = request(&["quiet"]);
        req.budget = 20;
        let d = deck(&all, &req, 1, &HashMap::new());
        assert!(!d.scored.is_empty(), "{:?}", d.blame);
        let note = d.relaxed.expect("the fallback must be announced");
        assert!(note.contains("closest"), "{note}");
    }
}
