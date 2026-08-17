use crate::db::now;
use crate::model::{Candidate, Reason};
use std::collections::HashMap;

pub struct Context {
    pub budget: i64,
    pub mood: String,
    pub appetite: String,
    pub taste: HashMap<String, f64>,
    pub seed: u64,
}

#[derive(Debug, Clone)]
pub struct Scored {
    pub total: f64,
    pub reasons: Vec<Reason>,
    pub caution: Option<String>,
    pub fit: f64,
}

struct MoodTarget {
    pace: (f64, f64),
    brain: (f64, f64),
    story: (f64, f64),
    social: (f64, f64),
    chill: (f64, f64),
}

fn mood_target(mood: &str) -> MoodTarget {
    match mood {
        "fried" => MoodTarget {
            pace: (0.45, 0.5),
            brain: (0.12, 1.5),
            story: (0.30, 0.4),
            social: (0.10, 0.3),
            chill: (0.85, 1.1),
        },
        "mellow" => MoodTarget {
            pace: (0.28, 0.7),
            brain: (0.42, 0.7),
            story: (0.68, 0.7),
            social: (0.15, 0.3),
            chill: (0.82, 1.2),
        },
        "sharp" => MoodTarget {
            pace: (0.50, 0.4),
            brain: (0.92, 1.5),
            story: (0.60, 0.5),
            social: (0.20, 0.2),
            chill: (0.25, 0.5),
        },
        "restless" => MoodTarget {
            pace: (0.93, 1.5),
            brain: (0.30, 0.6),
            story: (0.20, 0.4),
            social: (0.35, 0.3),
            chill: (0.10, 0.7),
        },
        "social" => MoodTarget {
            pace: (0.60, 0.3),
            brain: (0.40, 0.2),
            story: (0.25, 0.2),
            social: (0.92, 2.2),
            chill: (0.45, 0.2),
        },
        _ => MoodTarget {
            pace: (0.5, 0.2),
            brain: (0.5, 0.2),
            story: (0.5, 0.2),
            social: (0.3, 0.2),
            chill: (0.5, 0.2),
        },
    }
}

fn mood_phrase(mood: &str) -> &'static str {
    match mood {
        "fried" => "nothing to learn, nothing to lose — right for a fried brain",
        "mellow" => "unhurried and low-stakes, which is what you asked for",
        "sharp" => "it will actually make you think",
        "restless" => "immediate, loud, and it starts in seconds",
        "social" => "built for two — someone else can be in the room",
        _ => "matches the mood you picked",
    }
}

/// How well a game's typical sitting fits the time on the clock. Overshooting
/// the budget is punished far harder than undershooting it.
fn time_fit(est: i64, commitment: f64, budget: i64) -> f64 {
    if budget <= 0 {
        let d = (est as f64 - 90.0).abs() / 150.0;
        return (0.82 + 0.18 * (1.0 - d)).clamp(0.4, 1.0);
    }
    let b = budget as f64;
    let e = est as f64;
    let base = if e <= b {
        1.0 - 0.35 * ((b - e) / b).powf(1.5)
    } else {
        (-1.9 * ((e - b) / (b * 0.6 + 20.0))).exp()
    };
    let squeeze = if budget < 60 && commitment > 0.65 {
        0.75 - (commitment - 0.65)
    } else {
        1.0
    };
    (base * squeeze).clamp(0.0, 1.0)
}

/// A game we know nothing about should not be confidently rejected on time, nor
/// confidently recommended. Low confidence pulls the fit term toward neutral.
fn blend_toward_neutral(fit: f64, confidence: f64) -> f64 {
    const NEUTRAL: f64 = 0.72;
    let trust = confidence.clamp(0.0, 1.0).powf(0.6);
    NEUTRAL + (fit - NEUTRAL) * trust
}

/// The argument for an abandoned game depends on how far in you got: twenty
/// minutes was never a trial, twenty hours was almost a finish.
fn unfinished_phrase(playtime_min: i64, seen_fraction: f64, since_days: f64) -> String {
    if playtime_min < 120 {
        format!(
            "You stopped {} in, {}. That was never a fair trial.",
            minutes_phrase(playtime_min),
            ago(since_days)
        )
    } else if seen_fraction > 0.45 {
        format!(
            "{} in and you walked away {}. You were nearly there.",
            minutes_phrase(playtime_min),
            ago(since_days)
        )
    } else {
        format!(
            "{} in, then {} of silence. It was going well.",
            minutes_phrase(playtime_min),
            duration(since_days)
        )
    }
}

fn days_since(ts: i64) -> f64 {
    if ts <= 0 {
        return 9999.0;
    }
    ((now() - ts).max(0) as f64) / 86400.0
}

fn splitmix(mut x: u64) -> f64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

pub fn taste_score(taste: &HashMap<String, f64>, c: &Candidate) -> f64 {
    let mut sum = 0.0;
    let mut n = 0.0;
    for g in &c.meta.genres {
        if let Some(w) = taste.get(&format!("genre:{g}")) {
            sum += *w;
            n += 1.0;
        }
    }
    for cat in c.meta.categories.iter().take(6) {
        if let Some(w) = taste.get(&format!("cat:{cat}")) {
            sum += *w * 0.5;
            n += 0.5;
        }
    }
    let elo_term = ((c.prefs.elo - 1500.0) / 220.0).tanh() * 0.6;
    if n == 0.0 {
        return elo_term;
    }
    ((sum / n).tanh() * 0.7 + elo_term).clamp(-1.0, 1.0)
}

/// The whole recommendation argument in one place: every term contributes both a
/// number and, when it is decisive, the sentence shown on the card.
pub fn score(c: &Candidate, ctx: &Context) -> Scored {
    let mut reasons: Vec<Reason> = vec![];
    let mut total = 0.0;

    let raw_fit = time_fit(c.shape.session_minutes, c.shape.commitment, ctx.budget);
    let fit = blend_toward_neutral(raw_fit, c.shape.confidence);
    let mut caution = None;
    total += 1.05 * fit;
    if ctx.budget > 0 && fit > 0.86 {
        reasons.push(Reason {
            weight: 1.05 * fit,
            kind: "fit".into(),
            text: format!(
                "{} — it fits the {} you have",
                capitalise(&c.shape.session_label),
                humanize_budget(ctx.budget)
            ),
        });
    } else if raw_fit < 0.45
        && ctx.budget > 0
        && c.shape.confidence >= 0.35
        && c.shape.session_minutes > ctx.budget
    {
        caution = Some(format!("wants {}", c.shape.session_label));
    }

    let t = mood_target(&ctx.mood);
    let axes = [
        (c.shape.pace, t.pace),
        (c.shape.brain, t.brain),
        (c.shape.story, t.story),
        (c.shape.social, t.social),
        (c.shape.chill, t.chill),
    ];
    let wsum: f64 = axes.iter().map(|(_, (_, w))| w).sum();
    let err: f64 = axes.iter().map(|(v, (tv, w))| w * (v - tv).abs()).sum();
    let mood = (1.0 - err / wsum).clamp(0.0, 1.0);
    total += 0.9 * mood;
    if mood > 0.72 {
        reasons.push(Reason {
            weight: 0.9 * mood,
            kind: "mood".into(),
            text: capitalise(mood_phrase(&ctx.mood)),
        });
    }

    let hours = c.game.playtime_min as f64 / 60.0;
    let since = days_since(c.game.last_played);
    let seen_fraction = if c.shape.completion_hours > 0.0 {
        (hours / c.shape.completion_hours).clamp(0.0, 1.5)
    } else {
        0.0
    };
    let untouched = c.game.playtime_min == 0;
    let barely_touched = c.game.playtime_min <= 10;

    let (appetite_v, appetite_reason) = match ctx.appetite.as_str() {
        "comfort" => {
            let v = (hours / 12.0).min(1.0) * 0.75
                + (c.history.loved as f64 * 0.25).min(0.25)
                + if c.game.playtime_2w > 0 { 0.15 } else { 0.0 };
            let v = v.min(1.0);
            let r = if c.history.loved > 0 {
                Some(format!(
                    "You've told this app you love it. {:.0} hours in.",
                    hours
                ))
            } else if hours >= 5.0 {
                Some(format!(
                    "{:.0} hours already. You know exactly what you're getting.",
                    hours
                ))
            } else {
                None
            };
            (if untouched { 0.0 } else { v }, r)
        }
        "unfinished" => {
            let window = ((seen_fraction - 0.02) / 0.35).clamp(0.0, 1.0)
                * (1.0 - ((seen_fraction - 0.55) / 0.45).clamp(0.0, 1.0));
            let staleness = (since / 120.0).min(1.0);
            let v = if c.game.playtime_min < 10 {
                0.15
            } else {
                (window * 0.7 + staleness * 0.3).clamp(0.0, 1.0)
            };
            let r = if c.game.playtime_min >= 10 && since > 45.0 {
                Some(unfinished_phrase(c.game.playtime_min, seen_fraction, since))
            } else {
                None
            };
            (v, r)
        }
        "stranger" => {
            let v = if untouched {
                1.0
            } else if barely_touched {
                0.7
            } else {
                0.05
            };
            let r = if untouched {
                Some(owned_since_phrase(c))
            } else if barely_touched {
                Some(format!(
                    "{} minutes, {}. You have not actually met this game.",
                    c.game.playtime_min,
                    ago(since)
                ))
            } else {
                None
            };
            (v, r)
        }
        _ => {
            let novelty = if untouched {
                0.75
            } else {
                (1.0 - seen_fraction).max(0.2)
            };
            (0.45 + 0.3 * novelty, None)
        }
    };
    total += 1.15 * appetite_v;
    if let Some(text) = appetite_reason {
        if appetite_v > 0.45 {
            reasons.push(Reason {
                weight: 1.15 * appetite_v,
                kind: format!("appetite:{}", ctx.appetite),
                text,
            });
        }
    }

    if c.history.ach_total > 0 && c.history.ach_unlocked > 0 {
        let pct = c.history.ach_unlocked as f64 / c.history.ach_total as f64;
        let left = c.history.ach_total - c.history.ach_unlocked;
        let momentum = if (0.35..0.97).contains(&pct) {
            1.0 - (pct - 0.75).abs()
        } else {
            0.0
        };
        total += 0.45 * momentum;
        if momentum > 0.6 && left <= 12 {
            reasons.push(Reason {
                weight: 0.45 * momentum,
                kind: "momentum".into(),
                text: format!(
                    "{:.0}% of the achievements done — {} left",
                    pct * 100.0,
                    left
                ),
            });
        }
    }

    let quality = if c.meta.metacritic > 0 {
        ((c.meta.metacritic as f64 - 60.0) / 40.0).clamp(0.0, 1.0)
    } else if c.meta.recommendations > 0 {
        ((c.meta.recommendations as f64).log10() / 6.0).clamp(0.0, 1.0)
    } else {
        0.35
    };
    total += 0.3 * quality;
    if untouched && c.meta.metacritic >= 85 {
        reasons.push(Reason {
            weight: 0.4,
            kind: "quality".into(),
            text: format!(
                "{} on Metacritic, and you have never once opened it",
                c.meta.metacritic
            ),
        });
    }

    let taste = taste_score(&ctx.taste, c);
    total += 0.5 * taste;
    if taste > 0.35 && c.prefs.duels >= 2 {
        reasons.push(Reason {
            weight: 0.5 * taste,
            kind: "taste".into(),
            text: format!(
                "You have picked this over {} other {} in a straight fight",
                c.prefs.duels,
                if c.prefs.duels == 1 { "game" } else { "games" }
            ),
        });
    }

    let pass_recency = if c.history.last_pass > 0 {
        0.6 * (-days_since(c.history.last_pass) / 2.5).exp()
    } else {
        0.0
    };
    let pass_fatigue = 0.06 * (c.history.passes.min(6) as f64);
    let dealt_recently = if c.history.last_dealt > 0 && days_since(c.history.last_dealt) < 0.25 {
        0.25
    } else {
        0.0
    };
    total -= pass_recency + pass_fatigue + dealt_recently;

    if c.history.last_verdict == "never" {
        total -= 2.5;
    }
    if c.history.bounces >= 2 && c.history.loved == 0 {
        total -= 0.3;
    }

    if !c.game.installed {
        total -= 0.18;
    }
    if c.prefs.pinned {
        total += 0.8;
        reasons.push(Reason {
            weight: 0.8,
            kind: "pinned".into(),
            text: "You pinned this. It stays in the deck until you play it.".into(),
        });
    }

    total +=
        0.14 * (splitmix(ctx.seed ^ (c.game.appid as u64).wrapping_mul(0x2545F491)) - 0.5) * 2.0;

    if c.shape.confidence < 0.25 {
        total -= 0.12;
    }

    reasons.retain(|r| r.weight > 0.0);
    reasons.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    reasons.truncate(3);

    Scored {
        total,
        reasons,
        caution,
        fit,
    }
}

pub fn capitalise(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_uppercase().collect::<String>() + ch.as_str(),
        None => String::new(),
    }
}

pub fn humanize_budget(b: i64) -> String {
    match b {
        0 => "open evening".into(),
        m if m < 60 => format!("{m} minutes"),
        60 => "hour".into(),
        90 => "hour and a half".into(),
        m if m >= 180 => "evening".into(),
        m => format!("{:.1} hours", m as f64 / 60.0),
    }
}

pub fn minutes_phrase(m: i64) -> String {
    match m {
        m if m < 90 => format!("{m} minutes"),
        m if m < 600 => format!("{:.1} hours", m as f64 / 60.0),
        m => format!("{} hours", m / 60),
    }
}

pub fn ago(days: f64) -> String {
    match days {
        d if d > 9000.0 => "at some point".into(),
        d if d < 1.5 => "yesterday".into(),
        d if d < 14.0 => format!("{} days ago", d.round() as i64),
        d if d < 60.0 => format!("{} weeks ago", (d / 7.0).round() as i64),
        d if d < 730.0 => format!("{} months ago", (d / 30.4).round() as i64),
        d => format!("{:.0} years ago", d / 365.25),
    }
}

/// Like `ago`, but as a bare span so it can sit mid-sentence.
pub fn duration(days: f64) -> String {
    match days {
        d if d < 2.0 => "a day".into(),
        d if d < 14.0 => format!("{} days", d.round() as i64),
        d if d < 60.0 => format!("{} weeks", (d / 7.0).round() as i64),
        d if d < 730.0 => format!("{} months", (d / 30.4).round() as i64),
        d => format!("{:.0} years", d / 365.25),
    }
}

fn owned_since_phrase(c: &Candidate) -> String {
    if c.meta.release_year > 0 {
        format!("Never launched. Not once since {}.", c.meta.release_year)
    } else {
        "Never launched. Not once.".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_budget_rejects_long_games() {
        assert!(time_fit(150, 0.9, 20) < 0.1);
        assert!(
            time_fit(25, 0.15, 20) > 0.7,
            "a five-minute overshoot must stay usable"
        );
        assert!(time_fit(20, 0.15, 20) > 0.95);
    }

    #[test]
    fn open_evening_accepts_anything_reasonable() {
        assert!(time_fit(30, 0.2, 0) > 0.4);
        assert!(time_fit(120, 0.8, 0) > 0.7);
    }

    #[test]
    fn undershooting_beats_overshooting() {
        let under = time_fit(30, 0.3, 90);
        let over = time_fit(150, 0.3, 90);
        assert!(under > over, "under {under} over {over}");
    }

    #[test]
    fn heavy_games_are_squeezed_out_of_short_slots() {
        let light = time_fit(40, 0.2, 45);
        let heavy = time_fit(40, 0.95, 45);
        assert!(light > heavy);
    }
}
