use crate::db::{now, Db};
use crate::engine::{load, score};
use crate::model::{Candidate, Card, DealRequest, Hand, Slot};
use anyhow::{anyhow, Result};
use rusqlite::params;
use std::collections::HashSet;

pub struct Deck<'a> {
    pub scored: Vec<(&'a Candidate, score::Scored)>,
}

fn seed_from(req: &DealRequest, salt: u64) -> u64 {
    let mut s = now() as u64 / 30;
    s = s.wrapping_mul(0x9E3779B97F4A7C15) ^ (req.budget as u64).wrapping_mul(31);
    for b in req.mood.bytes().chain(req.appetite.bytes()) {
        s = s.rotate_left(7) ^ b as u64;
    }
    s ^ salt
}

fn primary_genre(c: &Candidate) -> String {
    c.meta
        .genres
        .iter()
        .find(|g| *g != "Indie" && *g != "Early Access" && *g != "Free to Play")
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
        Slot::Unfinished => {
            c.game.playtime_min >= 10
                && seen_fraction(c) < 0.7
                && days_since(c.game.last_played) >= 21.0
        }
        Slot::Stranger => c.game.playtime_min <= 10,
        _ => true,
    }
}

/// Builds the scored deck for a given context, applying hard filters that no
/// amount of score should override.
pub fn deck<'a>(
    all: &'a [Candidate],
    req: &DealRequest,
    seed: u64,
    taste: &std::collections::HashMap<String, f64>,
) -> Deck<'a> {
    let ctx = score::Context {
        budget: req.budget,
        mood: req.mood.clone(),
        appetite: req.appetite.clone(),
        taste: taste.clone(),
        seed,
    };
    let excluded: HashSet<i64> = req.exclude.iter().copied().collect();
    let mut pool: Vec<&Candidate> = load::eligible(all)
        .into_iter()
        .filter(|c| !excluded.contains(&c.game.appid))
        .collect();

    if req.mood == "social" {
        let social: Vec<&Candidate> = pool
            .iter()
            .copied()
            .filter(|c| c.shape.social > 0.35)
            .collect();
        if social.len() >= 6 {
            pool = social;
        }
    }

    let mut scored: Vec<(&Candidate, score::Scored)> = pool
        .into_iter()
        .map(|c| (c, score::score(c, &ctx)))
        .collect();

    if req.budget > 0 {
        let fits: Vec<(&Candidate, score::Scored)> = scored
            .iter()
            .filter(|(_, s)| s.fit >= 0.22)
            .map(|(c, s)| (*c, s.clone()))
            .collect();
        if fits.len() >= 8 {
            scored = fits;
        }
    }

    scored.sort_by(|a, b| {
        b.1.total
            .partial_cmp(&a.1.total)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Deck { scored }
}

/// Picks one card per emotional slot rather than the global top three, so the
/// player compares kinds of evening instead of comparing 400 interchangeable games.
fn pick_triad(deck: &Deck, appetite: &str) -> Vec<(Slot, usize)> {
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

    let mut taken: HashSet<i64> = HashSet::new();
    let mut genres: HashSet<String> = HashSet::new();
    let mut out: Vec<(Slot, usize)> = vec![];

    for slot in order {
        let mut fallback: Option<usize> = None;
        let mut chosen: Option<usize> = None;
        for (i, (c, _)) in deck.scored.iter().enumerate() {
            if taken.contains(&c.game.appid) || !in_slot(c, slot) {
                continue;
            }
            if fallback.is_none() {
                fallback = Some(i);
            }
            if !genres.contains(&primary_genre(c)) {
                chosen = Some(i);
                break;
            }
        }
        if let Some(i) = chosen.or(fallback) {
            taken.insert(deck.scored[i].0.game.appid);
            genres.insert(primary_genre(deck.scored[i].0));
            out.push((slot, i));
        }
    }

    while out.len() < 3 {
        let mut added = false;
        for (i, (c, _)) in deck.scored.iter().enumerate() {
            if taken.contains(&c.game.appid) {
                continue;
            }
            if genres.contains(&primary_genre(c)) && deck.scored.len() > out.len() + 6 {
                continue;
            }
            taken.insert(c.game.appid);
            genres.insert(primary_genre(c));
            out.push((Slot::Wildcard, i));
            added = true;
            break;
        }
        if !added {
            break;
        }
    }
    out
}

/// A card's headline must agree with the role it was dealt into. A comfort-food
/// argument under "Unfinished business" reads as a bug, even when the score is right.
fn suits_slot(kind: &str, slot: Slot) -> bool {
    match kind {
        "appetite:comfort" => slot == Slot::SureThing || slot == Slot::Wildcard,
        "appetite:unfinished" => slot == Slot::Unfinished || slot == Slot::Wildcard,
        "appetite:stranger" => slot == Slot::Stranger || slot == Slot::Wildcard,
        _ => true,
    }
}

fn to_card(c: &Candidate, s: &score::Scored, slot: Slot) -> Card {
    let verdict = s
        .reasons
        .iter()
        .find(|r| suits_slot(&r.kind, slot))
        .map(|r| r.text.clone())
        .unwrap_or_else(|| default_verdict(c, slot));
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
        session_label: c.shape.session_label.clone(),
        session_minutes: c.shape.session_minutes,
        playtime_min: c.game.playtime_min,
        last_played: c.game.last_played,
        installed: c.game.installed,
        genres: c.meta.genres.clone(),
        metacritic: c.meta.metacritic,
        short_desc: c.meta.short_desc.clone(),
        achievements_total: c.history.ach_total,
        release_year: c.meta.release_year,
    }
}

fn default_verdict(c: &Candidate, slot: Slot) -> String {
    match slot {
        Slot::Stranger => {
            if c.game.playtime_min == 0 {
                "You bought it. You have never once opened it.".into()
            } else {
                format!(
                    "{} minutes, once. That is a rounding error, not an opinion.",
                    c.game.playtime_min
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
        _ => format!(
            "{}. No further argument.",
            score::capitalise(&c.shape.session_label)
        ),
    }
}

pub fn hand_id(seed: u64) -> String {
    format!(
        "{:016x}",
        seed ^ (now() as u64).wrapping_mul(0x2545F4914F6CDD1D)
    )
}

pub fn deal(db: &Db, req: &DealRequest) -> Result<Hand> {
    let all = load::candidates(db)?;
    let taste = load::taste_map(db)?;
    let mut req = req.clone();
    if req.mood.is_empty() {
        req.mood = "mellow".into();
    }
    if req.appetite.is_empty() {
        req.appetite = "surprise".into();
    }

    let rerolls = match &req.reroll_of {
        Some(prev) => previous_rerolls(db, prev)? + 1,
        None => 0,
    };
    let seed = seed_from(&req, rerolls.wrapping_mul(7919) as u64);
    let deck = deck(&all, &req, seed, &taste);
    if deck.scored.is_empty() {
        return Err(anyhow!(
            "No games left to deal. Sync your library, or lift a few amnesties."
        ));
    }

    let picks = pick_triad(&deck, &req.appetite);
    let cards: Vec<Card> = picks
        .iter()
        .map(|(slot, i)| {
            let (c, s) = &deck.scored[*i];
            to_card(c, s, *slot)
        })
        .collect();

    let id = hand_id(seed);
    let json = serde_json::to_string(&cards)?;
    db.with(|c| {
        c.execute(
            "DELETE FROM hands WHERE ts < ?1",
            params![now() - 30 * 86400],
        )?;
        c.execute(
            "INSERT INTO hands(id,ts,budget,mood,appetite,cards,passes,rerolls,reroll_of)
             VALUES(?1,?2,?3,?4,?5,?6,0,?7,?8)",
            params![
                id,
                now(),
                req.budget,
                req.mood,
                req.appetite,
                json,
                rerolls,
                req.reroll_of
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

pub fn load_hand(db: &Db, id: &str) -> Result<Hand> {
    db.with(|c| {
        let (ts_budget, mood, appetite, json, passes, rerolls): (
            i64,
            String,
            String,
            String,
            i64,
            i64,
        ) = c.query_row(
            "SELECT budget, mood, appetite, cards, passes, rerolls FROM hands WHERE id=?1",
            params![id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )?;
        let cards: Vec<Card> = serde_json::from_str(&json)?;
        let allowed = 2;
        Ok(Hand {
            id: id.to_string(),
            passes_allowed: allowed,
            locked: cards.len() <= 1,
            cards,
            passes_used: passes,
            rerolls,
            budget: ts_budget,
            mood,
            appetite,
            note: reroll_note(rerolls),
        })
    })
}

/// Removes a card from a live hand. The last card standing is not removable —
/// that is the entire point of the exercise.
pub fn pass(db: &Db, id: &str, appid: i64) -> Result<Hand> {
    let mut hand = load_hand(db, id)?;
    if hand.cards.len() <= 1 {
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
pub fn rebound(
    db: &Db,
    budget: i64,
    mood: &str,
    appetite: &str,
    exclude: Vec<i64>,
) -> Result<Card> {
    let all = load::candidates(db)?;
    let taste = load::taste_map(db)?;
    let req = DealRequest {
        budget,
        mood: mood.to_string(),
        appetite: appetite.to_string(),
        exclude,
        reroll_of: None,
    };
    let seed = seed_from(&req, 0xBEEF);
    let deck = deck(&all, &req, seed, &taste);
    let (c, s) = deck
        .scored
        .first()
        .ok_or_else(|| anyhow!("nothing left to suggest"))?;
    Ok(to_card(c, s, Slot::Rebound))
}
