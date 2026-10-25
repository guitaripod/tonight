use crate::model::Candidate;
use serde::{Deserialize, Serialize};

/// A circumstance of the evening that is not about taste: the room, the machine
/// or the hour decides it. Taste says what you want; a condition says what is
/// possible, so it is checked before anything is ranked.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ConditionDef {
    pub id: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub chip: &'static str,
    pub warn: &'static str,
}

pub const CONDITIONS: &[ConditionDef] = &[
    ConditionDef {
        id: "quiet",
        label: "Sound is taken",
        hint: "I'm listening to something else",
        chip: "plays without sound",
        warn: "leans on sound",
    },
    ConditionDef {
        id: "interrupt",
        label: "I'll be interrupted",
        hint: "I might get pulled away",
        chip: "pauses cleanly",
        warn: "won't wait for you",
    },
    ConditionDef {
        id: "couch",
        label: "Couch and controller",
        hint: "big screen, no keyboard",
        chip: "controller-ready",
        warn: "wants a keyboard",
    },
    ConditionDef {
        id: "room",
        label: "Someone's in the room",
        hint: "couch co-op or a good watch",
        chip: "room-friendly",
        warn: "a solo game",
    },
    ConditionDef {
        id: "friends",
        label: "Friends are online",
        hint: "co-op or a match together",
        chip: "plays with friends",
        warn: "a solo game",
    },
    ConditionDef {
        id: "offline",
        label: "No internet",
        hint: "nothing that phones home",
        chip: "works offline",
        warn: "needs internet",
    },
    ConditionDef {
        id: "installed",
        label: "No downloads",
        hint: "only what's installed",
        chip: "installed",
        warn: "needs a download",
    },
    ConditionDef {
        id: "gentle",
        label: "Nothing scary",
        hint: "not before bed",
        chip: "not scary",
        warn: "tense",
    },
];

pub fn find(id: &str) -> Option<&'static ConditionDef> {
    CONDITIONS.iter().find(|c| c.id == id)
}

/// Keeps only known condition ids, once each, in the order the table lists them.
pub fn normalize(ids: &[String]) -> Vec<&'static ConditionDef> {
    CONDITIONS
        .iter()
        .filter(|def| ids.iter().any(|id| id == def.id))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    Good,
    Okay,
    Poor,
}

/// How one game stands against one condition: a 0..1 fit, a verdict on it, and a
/// sentence a person can argue with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fit {
    pub id: String,
    pub chip: String,
    pub warn: String,
    pub value: f64,
    pub standing: Standing,
    pub text: String,
    #[serde(default)]
    pub strict: bool,
}

impl Fit {
    fn new(def: &ConditionDef, value: f64, text: String) -> Fit {
        let value = value.clamp(0.0, 1.0);
        Fit {
            id: def.id.into(),
            chip: def.chip.into(),
            warn: def.warn.into(),
            value,
            standing: standing(value),
            text,
            strict: false,
        }
    }

    fn strict(mut self) -> Fit {
        self.strict = true;
        self
    }

    /// Whether the game clears this condition at the given floor. Strict
    /// conditions are yes-or-no and never loosen.
    pub fn clears(&self, floor: f64) -> bool {
        if self.strict {
            self.value >= 0.5
        } else {
            self.value >= floor
        }
    }
}

/// The fit at which a game counts as working well, not merely working.
pub const GOOD: f64 = 0.7;

fn standing(value: f64) -> Standing {
    if value >= GOOD {
        Standing::Good
    } else if value >= 0.42 {
        Standing::Okay
    } else {
        Standing::Poor
    }
}

fn with_reasons(lead: &str, labels: &[String]) -> String {
    let why = labels
        .iter()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ");
    if why.is_empty() {
        lead.to_string()
    } else {
        format!("{lead}: {why}")
    }
}

/// How a game fares under one condition. Unknown ids yield nothing.
pub fn evaluate(c: &Candidate, id: &str) -> Option<Fit> {
    let def = find(id)?;
    let s = &c.shape;
    Some(match id {
        "quiet" => {
            let risk = s.sound.assume(0.62);
            let fit = 1.0 - risk.powf(1.3);
            let text = match standing(fit) {
                Standing::Good => with_reasons("Nothing in it needs the sound", &s.sound.low),
                Standing::Okay => {
                    with_reasons("Playable muted, but it leans on audio", &s.sound.high)
                }
                Standing::Poor => with_reasons("It needs its sound", &s.sound.high),
            };
            Fit::new(def, fit, text)
        }
        "interrupt" => {
            let fit = s.pause.assume(0.45);
            let text = match standing(fit) {
                Standing::Good => {
                    with_reasons("You can drop it mid-way and lose nothing", &s.pause.high)
                }
                Standing::Okay => with_reasons(
                    "Pausable, though a run or a save point may cost you",
                    &s.pause.low,
                ),
                Standing::Poor => with_reasons("It will not wait for you", &s.pause.low),
            };
            Fit::new(def, fit, text)
        }
        "couch" => {
            let fit = s.controller.assume(0.4);
            let text = match standing(fit) {
                Standing::Good => with_reasons("Made for a gamepad", &s.controller.high),
                Standing::Okay => with_reasons(
                    "Works on a controller, with some friction",
                    &s.controller.low,
                ),
                Standing::Poor => with_reasons("Wants a keyboard and mouse", &s.controller.low),
            };
            Fit::new(def, fit, text)
        }
        "room" => {
            let shared = s.couch.assume(0.2);
            let watchable = 0.55 * (0.6 * s.story + 0.4 * (1.0 - s.pace));
            let fit = shared.max(watchable);
            let text = if shared >= 0.6 {
                with_reasons("Built for one screen and two people", &s.couch.high)
            } else if watchable >= 0.4 {
                "A good one to watch together, even if only one of you plays".to_string()
            } else {
                "Solo and twitchy, which is poor company".to_string()
            };
            Fit::new(def, fit, text)
        }
        "friends" => {
            let social = s.social.max(0.0);
            let reachable = if s.online.assume(0.3) >= 0.45 {
                1.0
            } else {
                0.4
            };
            let fit = social.powf(0.8) * reachable;
            let text = match standing(fit) {
                Standing::Good => "Built to play with other people online".to_string(),
                Standing::Okay => "Has a multiplayer side, but it is not the point".to_string(),
                Standing::Poor => "A solo game; your friends would just watch".to_string(),
            };
            Fit::new(def, fit, text)
        }
        "offline" => {
            let fit = 1.0 - s.online.assume(0.3);
            let text = match standing(fit) {
                Standing::Good => with_reasons("Runs without a connection", &s.online.low),
                Standing::Okay => with_reasons("Mostly offline, with online parts", &s.online.high),
                Standing::Poor => with_reasons("Needs a connection to play", &s.online.high),
            };
            Fit::new(def, fit, text)
        }
        "installed" => {
            let (fit, text) = if c.game.installed {
                (1.0, "Already installed".to_string())
            } else {
                (
                    0.0,
                    "Not installed, so it needs a download first".to_string(),
                )
            };
            Fit::new(def, fit, text).strict()
        }
        "gentle" => {
            let fit = 1.0 - (s.scare.assume(0.12) * 1.25).min(1.0);
            let text = match standing(fit) {
                Standing::Good => "Nothing frightening in it".to_string(),
                Standing::Okay => with_reasons("Tense in places", &s.scare.high),
                Standing::Poor => with_reasons("Genuinely frightening", &s.scare.high),
            };
            Fit::new(def, fit, text)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::shape::{self, Personal};
    use crate::model::{Game, History, Meta, Prefs};

    fn candidate(name: &str, tags: &[&str], categories: &[&str], installed: bool) -> Candidate {
        let game = Game {
            appid: 1,
            name: name.into(),
            playtime_min: 0,
            playtime_2w: 0,
            last_played: 0,
            installed,
            size_bytes: 0,
        };
        let meta = Meta {
            kind: "game".into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            categories: categories.iter().map(|s| s.to_string()).collect(),
            genres: vec!["Action".into()],
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

    #[test]
    fn sound_taken_loves_turn_based_and_loathes_horror() {
        let chess = candidate(
            "Tactics",
            &["Turn-Based Strategy", "Strategy", "Singleplayer"],
            &["Single-player", "Save Anytime"],
            true,
        );
        let scary = candidate(
            "Dread",
            &["Horror", "Atmospheric", "Psychological Horror"],
            &["Single-player"],
            true,
        );
        let a = evaluate(&chess, "quiet").unwrap();
        let b = evaluate(&scary, "quiet").unwrap();
        assert_eq!(a.standing, Standing::Good, "{}", a.text);
        assert_eq!(b.standing, Standing::Poor, "{}", b.text);
        assert!(!b.clears(0.3));
        assert!(a.text.contains("Turn-Based"), "{}", a.text);
    }

    #[test]
    fn an_unknown_game_is_not_assumed_quiet() {
        let blank = candidate("Mystery", &[], &[], true);
        let fit = evaluate(&blank, "quiet").unwrap();
        assert!(fit.value < 0.7, "got {}", fit.value);
    }

    #[test]
    fn online_matches_fail_the_interruption_test() {
        let match_game = candidate(
            "Arena",
            &["Multiplayer", "PvP", "Competitive", "FPS"],
            &["Multi-player", "Online PvP"],
            true,
        );
        let fit = evaluate(&match_game, "interrupt").unwrap();
        assert_eq!(fit.standing, Standing::Poor, "{}", fit.text);
        let offline = evaluate(&match_game, "offline").unwrap();
        assert_eq!(offline.standing, Standing::Poor, "{}", offline.text);
    }

    #[test]
    fn installed_is_yes_or_no_and_never_loosens() {
        let missing = candidate("Elsewhere", &[], &[], false);
        let fit = evaluate(&missing, "installed").unwrap();
        assert!(!fit.clears(0.0));
        let here = candidate("Here", &[], &[], true);
        assert!(evaluate(&here, "installed").unwrap().clears(0.9));
    }

    #[test]
    fn gentle_rules_out_horror_and_keeps_cozy() {
        let cozy = candidate("Garden", &["Cozy", "Relaxing", "Farming Sim"], &[], true);
        let horror = candidate("Nightmare", &["Horror", "Survival Horror"], &[], true);
        assert_eq!(evaluate(&cozy, "gentle").unwrap().standing, Standing::Good);
        assert_eq!(
            evaluate(&horror, "gentle").unwrap().standing,
            Standing::Poor
        );
    }

    #[test]
    fn unknown_condition_ids_are_ignored() {
        let c = candidate("Any", &[], &[], true);
        assert!(evaluate(&c, "nonsense").is_none());
        let ids = vec![
            "quiet".to_string(),
            "nonsense".to_string(),
            "quiet".to_string(),
        ];
        assert_eq!(normalize(&ids).len(), 1);
    }
}
