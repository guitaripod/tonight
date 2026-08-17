use crate::model::{Game, Meta};
use serde::Serialize;

/// The physical and emotional shape of a play session, derived from storefront
/// metadata. Every number is on 0..1 except the minute/hour fields.
#[derive(Debug, Clone, Serialize)]
pub struct Shape {
    pub session_minutes: i64,
    pub warmup_minutes: i64,
    pub completion_hours: f64,
    pub pace: f64,
    pub brain: f64,
    pub story: f64,
    pub social: f64,
    pub chill: f64,
    pub commitment: f64,
    pub confidence: f64,
    pub session_label: String,
    pub archetype: String,
}

impl Default for Shape {
    fn default() -> Self {
        Shape {
            session_minutes: 60,
            warmup_minutes: 10,
            completion_hours: 12.0,
            pace: 0.5,
            brain: 0.5,
            story: 0.4,
            social: 0.2,
            chill: 0.4,
            commitment: 0.5,
            confidence: 0.1,
            session_label: "an hour or so".into(),
            archetype: "unknown".into(),
        }
    }
}

struct Profile {
    session: i64,
    warmup: i64,
    completion: f64,
    pace: f64,
    brain: f64,
    story: f64,
    social: f64,
    chill: f64,
    commitment: f64,
    archetype: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn p(
    archetype: &'static str,
    session: i64,
    warmup: i64,
    completion: f64,
    pace: f64,
    brain: f64,
    story: f64,
    social: f64,
    chill: f64,
    commitment: f64,
) -> Profile {
    Profile {
        session,
        warmup,
        completion,
        pace,
        brain,
        story,
        social,
        chill,
        commitment,
        archetype,
    }
}

/// Description/name keywords that pin a game's shape far more precisely than its
/// Steam genre does. Checked longest-first so "grand strategy" beats "strategy".
fn keyword_profiles() -> &'static [(&'static str, Profile)] {
    static TABLE: &[(&str, Profile)] = &[
        (
            "grand strategy",
            p(
                "grand strategy",
                150,
                40,
                120.0,
                0.15,
                0.98,
                0.45,
                0.15,
                0.15,
                0.95,
            ),
        ),
        (
            "4x strategy",
            p("4X", 140, 30, 60.0, 0.2, 0.95, 0.3, 0.15, 0.2, 0.9),
        ),
        (
            "city builder",
            p(
                "city builder",
                110,
                20,
                40.0,
                0.2,
                0.75,
                0.2,
                0.05,
                0.6,
                0.8,
            ),
        ),
        (
            "city-building",
            p(
                "city builder",
                110,
                20,
                40.0,
                0.2,
                0.75,
                0.2,
                0.05,
                0.6,
                0.8,
            ),
        ),
        (
            "colony sim",
            p("colony sim", 130, 30, 70.0, 0.2, 0.9, 0.35, 0.05, 0.4, 0.9),
        ),
        (
            "management",
            p(
                "management sim",
                90,
                15,
                30.0,
                0.25,
                0.7,
                0.2,
                0.05,
                0.6,
                0.7,
            ),
        ),
        (
            "tycoon",
            p("tycoon", 100, 15, 35.0, 0.2, 0.65, 0.15, 0.05, 0.65, 0.7),
        ),
        (
            "visual novel",
            p(
                "visual novel",
                60,
                5,
                20.0,
                0.05,
                0.35,
                0.98,
                0.02,
                0.7,
                0.4,
            ),
        ),
        (
            "walking simulator",
            p("walking sim", 60, 3, 4.0, 0.1, 0.3, 0.95, 0.02, 0.85, 0.2),
        ),
        (
            "point-and-click",
            p("adventure", 55, 8, 10.0, 0.15, 0.65, 0.85, 0.05, 0.6, 0.4),
        ),
        (
            "point and click",
            p("adventure", 55, 8, 10.0, 0.15, 0.65, 0.85, 0.05, 0.6, 0.4),
        ),
        (
            "metroidvania",
            p(
                "metroidvania",
                55,
                12,
                15.0,
                0.65,
                0.5,
                0.5,
                0.02,
                0.35,
                0.55,
            ),
        ),
        (
            "roguelike deckbuilder",
            p("run-based", 35, 6, 25.0, 0.35, 0.8, 0.15, 0.02, 0.4, 0.15),
        ),
        (
            "deckbuilder",
            p("run-based", 35, 8, 25.0, 0.3, 0.8, 0.15, 0.05, 0.45, 0.2),
        ),
        (
            "deck-building",
            p("run-based", 35, 8, 25.0, 0.3, 0.8, 0.15, 0.05, 0.45, 0.2),
        ),
        (
            "roguelike",
            p("run-based", 32, 5, 30.0, 0.75, 0.6, 0.15, 0.05, 0.2, 0.15),
        ),
        (
            "rogue-like",
            p("run-based", 32, 5, 30.0, 0.75, 0.6, 0.15, 0.05, 0.2, 0.15),
        ),
        (
            "roguelite",
            p("run-based", 32, 5, 30.0, 0.75, 0.6, 0.15, 0.05, 0.2, 0.15),
        ),
        (
            "rogue-lite",
            p("run-based", 32, 5, 30.0, 0.75, 0.6, 0.15, 0.05, 0.2, 0.15),
        ),
        (
            "run-based",
            p("run-based", 30, 5, 25.0, 0.7, 0.55, 0.1, 0.05, 0.25, 0.15),
        ),
        (
            "dungeon crawler",
            p(
                "dungeon crawler",
                55,
                10,
                25.0,
                0.6,
                0.6,
                0.3,
                0.2,
                0.2,
                0.5,
            ),
        ),
        (
            "hack and slash",
            p("action", 45, 5, 20.0, 0.92, 0.35, 0.3, 0.2, 0.1, 0.3),
        ),
        (
            "beat 'em up",
            p("arcade", 30, 3, 8.0, 0.95, 0.25, 0.15, 0.45, 0.1, 0.15),
        ),
        (
            "shoot 'em up",
            p("arcade", 25, 3, 10.0, 0.98, 0.35, 0.05, 0.1, 0.05, 0.1),
        ),
        (
            "boomer shooter",
            p("arcade", 35, 3, 12.0, 0.97, 0.3, 0.15, 0.05, 0.05, 0.15),
        ),
        (
            "arena shooter",
            p(
                "match-based",
                30,
                5,
                100.0,
                0.98,
                0.35,
                0.05,
                0.75,
                0.05,
                0.15,
            ),
        ),
        (
            "tactical shooter",
            p(
                "match-based",
                45,
                20,
                200.0,
                0.8,
                0.7,
                0.05,
                0.85,
                0.02,
                0.4,
            ),
        ),
        (
            "extraction shooter",
            p("match-based", 50, 25, 300.0, 0.75, 0.7, 0.1, 0.8, 0.02, 0.5),
        ),
        (
            "stealth",
            p("stealth", 65, 12, 18.0, 0.4, 0.75, 0.55, 0.05, 0.1, 0.6),
        ),
        (
            "life sim",
            p("cozy", 70, 8, 50.0, 0.15, 0.35, 0.45, 0.1, 0.9, 0.5),
        ),
        (
            "puzzle platformer",
            p("puzzle", 35, 4, 8.0, 0.5, 0.7, 0.25, 0.05, 0.4, 0.2),
        ),
        (
            "card game",
            p("card game", 35, 10, 60.0, 0.2, 0.75, 0.1, 0.5, 0.4, 0.25),
        ),
        (
            "bullet hell",
            p("arcade", 25, 3, 12.0, 0.98, 0.4, 0.05, 0.05, 0.05, 0.1),
        ),
        (
            "twin-stick",
            p("arcade", 25, 3, 10.0, 0.95, 0.3, 0.05, 0.15, 0.1, 0.1),
        ),
        (
            "battle royale",
            p("match-based", 35, 5, 500.0, 0.9, 0.4, 0.05, 0.9, 0.1, 0.2),
        ),
        (
            "hero shooter",
            p("match-based", 40, 5, 500.0, 0.95, 0.4, 0.05, 0.9, 0.05, 0.2),
        ),
        (
            "moba",
            p(
                "match-based",
                45,
                20,
                800.0,
                0.85,
                0.75,
                0.05,
                0.9,
                0.02,
                0.35,
            ),
        ),
        (
            "auto battler",
            p("match-based", 35, 8, 100.0, 0.3, 0.7, 0.05, 0.5, 0.35, 0.15),
        ),
        (
            "soulslike",
            p(
                "hard action",
                70,
                20,
                40.0,
                0.85,
                0.7,
                0.4,
                0.15,
                0.02,
                0.75,
            ),
        ),
        (
            "souls-like",
            p(
                "hard action",
                70,
                20,
                40.0,
                0.85,
                0.7,
                0.4,
                0.15,
                0.02,
                0.75,
            ),
        ),
        (
            "immersive sim",
            p(
                "immersive sim",
                80,
                20,
                25.0,
                0.55,
                0.85,
                0.7,
                0.05,
                0.2,
                0.7,
            ),
        ),
        (
            "open world",
            p("open world", 90, 20, 45.0, 0.55, 0.5, 0.6, 0.1, 0.4, 0.7),
        ),
        (
            "open-world",
            p("open world", 90, 20, 45.0, 0.55, 0.5, 0.6, 0.1, 0.4, 0.7),
        ),
        (
            "survival",
            p("survival", 100, 25, 50.0, 0.5, 0.65, 0.25, 0.35, 0.15, 0.85),
        ),
        (
            "crafting",
            p("crafting", 95, 20, 45.0, 0.35, 0.6, 0.2, 0.3, 0.5, 0.8),
        ),
        (
            "farming",
            p("cozy", 75, 10, 60.0, 0.15, 0.35, 0.4, 0.15, 0.95, 0.5),
        ),
        (
            "cozy",
            p("cozy", 50, 5, 15.0, 0.1, 0.25, 0.4, 0.1, 0.98, 0.3),
        ),
        (
            "relaxing",
            p("cozy", 45, 4, 10.0, 0.1, 0.25, 0.3, 0.05, 0.95, 0.25),
        ),
        (
            "wholesome",
            p("cozy", 45, 4, 10.0, 0.1, 0.25, 0.45, 0.1, 0.95, 0.25),
        ),
        (
            "horror",
            p("horror", 60, 8, 8.0, 0.6, 0.4, 0.75, 0.1, 0.02, 0.6),
        ),
        (
            "puzzle",
            p("puzzle", 30, 3, 8.0, 0.1, 0.85, 0.15, 0.05, 0.5, 0.2),
        ),
        (
            "platformer",
            p("platformer", 35, 3, 10.0, 0.85, 0.35, 0.2, 0.1, 0.25, 0.25),
        ),
        (
            "rhythm",
            p("rhythm", 25, 2, 12.0, 0.95, 0.45, 0.05, 0.05, 0.3, 0.1),
        ),
        (
            "turn-based tactics",
            p("tactics", 70, 15, 30.0, 0.15, 0.9, 0.35, 0.05, 0.3, 0.7),
        ),
        (
            "turn-based",
            p("turn-based", 65, 12, 30.0, 0.15, 0.85, 0.4, 0.05, 0.4, 0.6),
        ),
        (
            "real-time strategy",
            p("RTS", 60, 15, 25.0, 0.7, 0.85, 0.3, 0.3, 0.1, 0.6),
        ),
        (
            "tower defense",
            p(
                "tower defense",
                35,
                6,
                15.0,
                0.4,
                0.7,
                0.05,
                0.05,
                0.45,
                0.2,
            ),
        ),
        (
            "idle",
            p("idle", 15, 1, 100.0, 0.05, 0.1, 0.05, 0.02, 0.95, 0.05),
        ),
        (
            "clicker",
            p("idle", 15, 1, 100.0, 0.05, 0.1, 0.05, 0.02, 0.9, 0.05),
        ),
        (
            "sandbox",
            p("sandbox", 90, 15, 60.0, 0.35, 0.5, 0.15, 0.3, 0.6, 0.6),
        ),
        (
            "story-driven",
            p("narrative", 70, 10, 15.0, 0.35, 0.45, 0.95, 0.05, 0.4, 0.55),
        ),
        (
            "story rich",
            p("narrative", 70, 10, 18.0, 0.35, 0.45, 0.95, 0.05, 0.4, 0.55),
        ),
        (
            "narrative",
            p("narrative", 65, 8, 12.0, 0.3, 0.45, 0.95, 0.05, 0.45, 0.5),
        ),
        (
            "mmorpg",
            p("MMO", 120, 30, 900.0, 0.4, 0.55, 0.35, 0.85, 0.25, 0.95),
        ),
        (
            "racing",
            p("racing", 35, 4, 15.0, 0.9, 0.35, 0.1, 0.35, 0.2, 0.2),
        ),
        (
            "fighting game",
            p("fighting", 35, 15, 30.0, 0.95, 0.6, 0.1, 0.7, 0.05, 0.25),
        ),
        (
            "football",
            p("sports", 40, 6, 40.0, 0.75, 0.4, 0.1, 0.5, 0.2, 0.25),
        ),
        (
            "golf",
            p("sports", 35, 4, 15.0, 0.35, 0.4, 0.05, 0.5, 0.7, 0.2),
        ),
    ];
    TABLE
}

fn genre_profile(genre: &str) -> Option<Profile> {
    Some(match genre {
        "Massively Multiplayer" => p("MMO", 120, 30, 600.0, 0.5, 0.55, 0.3, 0.9, 0.2, 0.95),
        "Strategy" => p("strategy", 90, 20, 40.0, 0.3, 0.9, 0.3, 0.2, 0.25, 0.75),
        "RPG" => p("RPG", 90, 20, 45.0, 0.45, 0.65, 0.85, 0.15, 0.3, 0.8),
        "Simulation" => p("simulation", 80, 15, 35.0, 0.25, 0.7, 0.2, 0.1, 0.6, 0.7),
        "Sports" => p("sports", 40, 6, 30.0, 0.75, 0.4, 0.1, 0.5, 0.25, 0.25),
        "Racing" => p("racing", 35, 4, 18.0, 0.9, 0.35, 0.15, 0.35, 0.25, 0.2),
        "Action" => p("action", 50, 8, 18.0, 0.85, 0.4, 0.4, 0.25, 0.15, 0.4),
        "Adventure" => p("adventure", 65, 10, 15.0, 0.4, 0.5, 0.85, 0.08, 0.5, 0.5),
        "Casual" => p("casual", 30, 2, 8.0, 0.3, 0.3, 0.15, 0.1, 0.85, 0.15),
        "Indie" => p("indie", 45, 6, 12.0, 0.5, 0.5, 0.45, 0.1, 0.45, 0.35),
        "Free to Play" => p("free-to-play", 45, 8, 200.0, 0.7, 0.5, 0.15, 0.7, 0.2, 0.5),
        "Early Access" => p("early access", 60, 12, 25.0, 0.5, 0.6, 0.3, 0.3, 0.35, 0.55),
        _ => return None,
    })
}

fn blend(into: &mut Shape, prof: &Profile, w: f64) {
    let lerp = |a: f64, b: f64| a + (b - a) * w;
    into.session_minutes = lerp(into.session_minutes as f64, prof.session as f64).round() as i64;
    into.warmup_minutes = lerp(into.warmup_minutes as f64, prof.warmup as f64).round() as i64;
    into.completion_hours = lerp(into.completion_hours, prof.completion);
    into.pace = lerp(into.pace, prof.pace);
    into.brain = lerp(into.brain, prof.brain);
    into.story = lerp(into.story, prof.story);
    into.social = lerp(into.social, prof.social);
    into.chill = lerp(into.chill, prof.chill);
    into.commitment = lerp(into.commitment, prof.commitment);
    if w >= 0.5 {
        into.archetype = prof.archetype.to_string();
    }
}

/// Builds a session shape from metadata, then lets the player's own recorded
/// behaviour override the guess where evidence exists.
pub fn derive(
    game: &Game,
    meta: &Meta,
    session_override: Option<i64>,
    observed: Option<i64>,
) -> Shape {
    let mut s = Shape::default();
    let mut evidence = 0.0f64;

    let mut genres = meta.genres.iter();
    if let Some(prof) = genres.next().and_then(|g| genre_profile(g)) {
        blend(&mut s, &prof, 0.85);
        evidence += 0.5;
    }
    for g in genres {
        if let Some(prof) = genre_profile(g) {
            blend(&mut s, &prof, 0.22);
            evidence += 0.05;
        }
    }

    let hay = format!(
        "{} {} {}",
        game.name.to_ascii_lowercase(),
        meta.short_desc.to_ascii_lowercase(),
        meta.genres.join(" ").to_ascii_lowercase()
    );
    let mut keyword_hits = 0;
    for (kw, prof) in keyword_profiles() {
        if hay.contains(kw) {
            let w = if keyword_hits == 0 { 0.7 } else { 0.25 };
            blend(&mut s, prof, w);
            keyword_hits += 1;
            evidence += 0.3;
            if keyword_hits >= 3 {
                break;
            }
        }
    }

    let cats: Vec<String> = meta
        .categories
        .iter()
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let has = |needle: &str| cats.iter().any(|c| c.contains(needle));
    if has("co-op") || has("multi-player") || has("pvp") {
        s.social = (s.social + 0.35).min(1.0);
        evidence += 0.05;
    }
    if has("single-player") && !has("multi-player") {
        s.social *= 0.35;
    }
    if has("online pvp") && !has("single-player") {
        s.session_minutes = s.session_minutes.min(45);
        s.completion_hours = s.completion_hours.max(200.0);
    }
    if has("full controller support") {
        s.chill = (s.chill + 0.06).min(1.0);
    }
    if meta.metacritic >= 85 {
        s.completion_hours *= 1.1;
    }

    if let Some(m) = session_override.filter(|m| *m > 0) {
        s.session_minutes = m;
        s.confidence = 1.0;
        s.session_label = humanize(m);
        return s;
    }

    if let Some(obs) = observed.filter(|m| *m >= 8) {
        s.session_minutes =
            ((s.session_minutes as f64 * 0.35) + (obs as f64 * 0.65)).round() as i64;
        evidence += 0.6;
    }

    if game.playtime_min > 0 && s.completion_hours > 0.0 {
        let played_h = game.playtime_min as f64 / 60.0;
        if played_h > s.completion_hours {
            s.completion_hours = played_h * 1.15;
        }
    }

    s.session_minutes = s.session_minutes.clamp(10, 240);
    s.confidence = (evidence / 1.6).clamp(0.05, 1.0);
    s.session_label = humanize(s.session_minutes);
    s
}

pub fn humanize(minutes: i64) -> String {
    match minutes {
        m if m <= 20 => "a 20-minute hit".into(),
        m if m <= 35 => format!("a {m}-minute run"),
        m if m <= 50 => format!("a {m}-minute sitting"),
        m if m <= 75 => "about an hour".into(),
        m if m <= 105 => "an hour and a half".into(),
        m if m <= 150 => "a proper evening".into(),
        _ => "a whole night".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str) -> Game {
        Game {
            appid: 1,
            name: name.into(),
            playtime_min: 0,
            playtime_2w: 0,
            last_played: 0,
            installed: true,
            size_bytes: 0,
        }
    }

    fn meta(genres: &[&str], desc: &str) -> Meta {
        Meta {
            genres: genres.iter().map(|s| s.to_string()).collect(),
            short_desc: desc.into(),
            categories: vec!["Single-player".into()],
            ..Default::default()
        }
    }

    #[test]
    fn roguelikes_are_short_sessions() {
        let s = derive(
            &game("Hades"),
            &meta(
                &["Action", "Indie"],
                "A rogue-LIKE dungeon crawler with fast runs",
            ),
            None,
            None,
        );
        assert!(s.session_minutes <= 45, "got {}", s.session_minutes);
        assert_eq!(s.archetype, "run-based");
    }

    #[test]
    fn grand_strategy_demands_an_evening() {
        let s = derive(
            &game("Crusader Kings III"),
            &meta(
                &["Strategy", "Simulation"],
                "A grand strategy role-playing game",
            ),
            None,
            None,
        );
        assert!(s.session_minutes >= 100, "got {}", s.session_minutes);
        assert!(s.brain > 0.8);
    }

    #[test]
    fn observed_sessions_beat_the_guess() {
        let s = derive(&game("Whatever"), &meta(&["Action"], ""), None, Some(20));
        assert!(s.session_minutes < 40, "got {}", s.session_minutes);
    }

    #[test]
    fn manual_override_wins_outright() {
        let s = derive(
            &game("Whatever"),
            &meta(&["Strategy"], "grand strategy"),
            Some(25),
            Some(200),
        );
        assert_eq!(s.session_minutes, 25);
        assert_eq!(s.confidence, 1.0);
    }

    #[test]
    fn single_player_games_are_not_social() {
        let s = derive(
            &game("Portal"),
            &meta(&["Action"], "puzzle game"),
            None,
            None,
        );
        assert!(s.social < 0.2, "got {}", s.social);
    }
}
