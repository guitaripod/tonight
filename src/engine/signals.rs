use crate::model::Meta;
use serde::Serialize;

/// The eleven things tonight knows about how a game feels to play. The first five
/// describe the evening it suits; the last six describe what it asks of the room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Pace,
    Brain,
    Story,
    Social,
    Chill,
    Sound,
    Pause,
    Scare,
    Online,
    Controller,
    Couch,
}

pub const AXES: [Axis; 11] = [
    Axis::Pace,
    Axis::Brain,
    Axis::Story,
    Axis::Social,
    Axis::Chill,
    Axis::Sound,
    Axis::Pause,
    Axis::Scare,
    Axis::Online,
    Axis::Controller,
    Axis::Couch,
];

/// One estimated trait: where the game sits on 0..1, how much evidence backs it,
/// and the strongest reasons in plain words.
#[derive(Debug, Clone, Serialize)]
pub struct Trait {
    pub value: f64,
    pub confidence: f64,
    pub because: Vec<String>,
    pub high: Vec<String>,
    pub low: Vec<String>,
}

impl Trait {
    pub fn guess(value: f64) -> Trait {
        Trait {
            value,
            confidence: 0.0,
            because: vec![],
            high: vec![],
            low: vec![],
        }
    }

    pub fn fixed(value: f64, because: &str) -> Trait {
        Trait {
            value,
            confidence: 1.0,
            because: vec![because.to_string()],
            high: if value >= 0.5 {
                vec![because.to_string()]
            } else {
                vec![]
            },
            low: if value < 0.5 {
                vec![because.to_string()]
            } else {
                vec![]
            },
        }
    }

    /// The value to plan around when the evidence is thin: low confidence pulls
    /// the estimate toward `fallback` so an unknown game is never mistaken for a
    /// safe one.
    pub fn assume(&self, fallback: f64) -> f64 {
        let trust = self.confidence.clamp(0.0, 1.0).powf(0.7);
        fallback + (self.value - fallback) * trust
    }
}

/// Weighted evidence for one axis. Every vote is a (value, weight) pair tagged
/// with the label that cast it.
#[derive(Debug, Clone, Default)]
pub struct Acc {
    sum: f64,
    weight: f64,
    voices: Vec<(f64, f64, String)>,
}

impl Acc {
    pub fn add(&mut self, value: f64, weight: f64, label: &str) {
        self.sum += value * weight;
        self.weight += weight;
        self.voices.push((value, weight, label.to_string()));
    }

    /// Blends the votes with a prior of the given strength. The confidence is the
    /// evidence alone, so a strong prior never poses as knowledge.
    pub fn settle(&self, prior: f64, prior_weight: f64) -> Trait {
        let total = self.weight + prior_weight;
        let value = if total > 0.0 {
            (self.sum + prior * prior_weight) / total
        } else {
            prior
        };
        let labelled = |keep: &dyn Fn(f64) -> bool| -> Vec<String> {
            let mut voices: Vec<&(f64, f64, String)> = self
                .voices
                .iter()
                .filter(|v| !v.2.is_empty() && keep(v.0))
                .collect();
            voices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let mut out: Vec<String> = vec![];
            for (_, _, label) in voices {
                if !out.contains(label) {
                    out.push(label.clone());
                }
                if out.len() == 3 {
                    break;
                }
            }
            out
        };
        let mut ranked: Vec<&(f64, f64, String)> =
            self.voices.iter().filter(|v| !v.2.is_empty()).collect();
        ranked.sort_by(|a, b| {
            let pull = |v: &&(f64, f64, String)| v.1 * (1.0 - (v.0 - value).abs());
            pull(b)
                .partial_cmp(&pull(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut because: Vec<String> = vec![];
        for (_, _, label) in ranked {
            if !because.contains(label) {
                because.push(label.clone());
            }
            if because.len() == 3 {
                break;
            }
        }
        Trait {
            value: value.clamp(0.0, 1.0),
            confidence: 1.0 - (-self.weight / 2.0).exp(),
            because,
            high: labelled(&|v| v >= 0.6),
            low: labelled(&|v| v <= 0.4),
        }
    }
}

pub struct Ballot {
    accs: [Acc; 11],
}

impl Default for Ballot {
    fn default() -> Self {
        Ballot {
            accs: std::array::from_fn(|_| Acc::default()),
        }
    }
}

impl Ballot {
    fn slot(axis: Axis) -> usize {
        AXES.iter().position(|a| *a == axis).unwrap_or(0)
    }

    pub fn cast(&mut self, axis: Axis, value: f64, weight: f64, label: &str) {
        self.accs[Self::slot(axis)].add(value, weight, label);
    }

    pub fn acc(&self, axis: Axis) -> &Acc {
        &self.accs[Self::slot(axis)]
    }

    pub fn settle(&self, axis: Axis, prior: f64, prior_weight: f64) -> Trait {
        self.acc(axis).settle(prior, prior_weight)
    }
}

type Votes = &'static [(Axis, f32, f32)];

use Axis::{Brain, Chill, Controller, Couch, Online, Pace, Pause, Scare, Social, Sound, Story};

const TURN_BASED: Votes = &[
    (Pause, 0.96, 1.3),
    (Pace, 0.08, 1.0),
    (Brain, 0.85, 0.8),
    (Sound, 0.1, 1.0),
    (Controller, 0.4, 0.4),
];
const SLOW_STRATEGY: Votes = &[
    (Pause, 0.88, 1.0),
    (Pace, 0.15, 0.7),
    (Brain, 0.9, 1.0),
    (Sound, 0.14, 1.0),
    (Controller, 0.3, 0.6),
    (Scare, 0.02, 0.4),
];
const IDLE: Votes = &[
    (Pause, 1.0, 1.5),
    (Pace, 0.03, 1.0),
    (Brain, 0.15, 0.8),
    (Sound, 0.05, 1.3),
    (Chill, 0.92, 0.9),
    (Scare, 0.0, 0.8),
];
const PUZZLE: Votes = &[
    (Pause, 0.95, 1.1),
    (Brain, 0.88, 1.0),
    (Pace, 0.12, 0.6),
    (Sound, 0.2, 0.9),
    (Chill, 0.6, 0.5),
    (Scare, 0.03, 0.4),
];
const TEXT_DRIVEN: Votes = &[
    (Story, 0.97, 1.4),
    (Pause, 1.0, 1.4),
    (Pace, 0.03, 1.0),
    (Sound, 0.3, 0.9),
    (Chill, 0.7, 0.6),
    (Scare, 0.02, 0.4),
];
const COZY: Votes = &[
    (Chill, 0.97, 1.5),
    (Pace, 0.1, 1.0),
    (Pause, 0.9, 1.0),
    (Scare, 0.0, 1.2),
    (Sound, 0.42, 0.7),
    (Brain, 0.3, 0.5),
];
const HORROR: Votes = &[
    (Scare, 0.95, 2.0),
    (Sound, 0.93, 1.4),
    (Chill, 0.03, 1.0),
    (Pause, 0.45, 0.6),
    (Story, 0.65, 0.4),
];
const RHYTHM: Votes = &[
    (Sound, 1.0, 2.5),
    (Pace, 0.9, 1.0),
    (Pause, 0.25, 0.8),
    (Controller, 0.7, 0.4),
];
const MUSIC_FORWARD: Votes = &[(Sound, 0.85, 1.0)];
const SOUNDTRACK: Votes = &[(Sound, 0.72, 0.55)];
const ATMOSPHERIC: Votes = &[(Sound, 0.74, 0.7), (Chill, 0.5, 0.3)];
const CINEMATIC: Votes = &[(Sound, 0.72, 0.7), (Story, 0.72, 0.6)];
const SHOOTER: Votes = &[
    (Sound, 0.8, 0.8),
    (Pace, 0.78, 0.6),
    (Chill, 0.1, 0.5),
    (Controller, 0.55, 0.3),
];
const TACTICAL_SHOOTER: Votes = &[
    (Sound, 0.92, 1.2),
    (Pace, 0.7, 0.6),
    (Brain, 0.65, 0.6),
    (Pause, 0.15, 0.8),
    (Online, 0.85, 0.6),
];
const STEALTH: Votes = &[
    (Sound, 0.9, 1.3),
    (Brain, 0.7, 0.6),
    (Pause, 0.6, 0.4),
    (Chill, 0.25, 0.4),
];
const COMPETITIVE: Votes = &[
    (Online, 0.95, 1.3),
    (Social, 0.85, 0.9),
    (Pause, 0.05, 1.5),
    (Sound, 0.85, 0.9),
    (Chill, 0.05, 0.8),
    (Pace, 0.85, 0.6),
];
const MASSIVELY: Votes = &[
    (Online, 1.0, 2.0),
    (Pause, 0.1, 1.5),
    (Social, 0.88, 1.0),
    (Sound, 0.55, 0.6),
];
const SOULSLIKE: Votes = &[
    (Pace, 0.72, 0.8),
    (Brain, 0.65, 0.5),
    (Chill, 0.04, 1.2),
    (Pause, 0.22, 1.2),
    (Sound, 0.78, 1.0),
    (Scare, 0.3, 0.4),
];
const FAST: Votes = &[
    (Pace, 0.92, 1.3),
    (Chill, 0.1, 0.7),
    (Brain, 0.3, 0.4),
    (Pause, 0.4, 0.4),
];
const ARCADE: Votes = &[
    (Pace, 0.88, 1.0),
    (Brain, 0.3, 0.6),
    (Pause, 0.45, 0.6),
    (Sound, 0.5, 0.5),
    (Chill, 0.12, 0.6),
    (Controller, 0.75, 0.6),
];
const ROGUELIKE: Votes = &[
    (Pause, 0.5, 0.7),
    (Sound, 0.3, 0.8),
    (Pace, 0.6, 0.4),
    (Controller, 0.65, 0.4),
];
const PLATFORMER: Votes = &[
    (Controller, 0.9, 1.2),
    (Pace, 0.7, 0.6),
    (Sound, 0.5, 0.6),
    (Pause, 0.5, 0.5),
    (Brain, 0.4, 0.3),
];
const STORY: Votes = &[
    (Story, 0.92, 1.4),
    (Sound, 0.62, 0.6),
    (Brain, 0.5, 0.2),
    (Pause, 0.75, 0.5),
];
const WALKING: Votes = &[
    (Story, 0.9, 1.4),
    (Pace, 0.07, 1.0),
    (Sound, 0.82, 1.2),
    (Pause, 0.92, 0.8),
    (Chill, 0.7, 0.6),
    (Scare, 0.1, 0.3),
];
const ADVENTURE_CLICK: Votes = &[
    (Pause, 0.96, 1.2),
    (Brain, 0.75, 0.8),
    (Pace, 0.07, 1.0),
    (Sound, 0.55, 0.9),
    (Story, 0.85, 0.7),
    (Controller, 0.3, 0.4),
];
const OPEN_WORLD: Votes = &[
    (Story, 0.6, 0.4),
    (Pace, 0.5, 0.3),
    (Sound, 0.62, 0.5),
    (Controller, 0.75, 0.5),
    (Pause, 0.7, 0.3),
];
const RPG: Votes = &[
    (Story, 0.75, 0.7),
    (Pause, 0.68, 0.4),
    (Sound, 0.58, 0.4),
    (Controller, 0.7, 0.4),
];
const JRPG: Votes = &[
    (Story, 0.85, 0.8),
    (Controller, 0.9, 0.8),
    (Pause, 0.7, 0.5),
    (Sound, 0.6, 0.5),
    (Pace, 0.3, 0.4),
];
const SURVIVAL: Votes = &[
    (Brain, 0.65, 0.5),
    (Chill, 0.3, 0.5),
    (Pause, 0.6, 0.4),
    (Sound, 0.6, 0.5),
    (Pace, 0.4, 0.3),
];
const EXPLORE: Votes = &[(Chill, 0.62, 0.5), (Sound, 0.6, 0.4), (Pace, 0.3, 0.3)];
const DRIVING: Votes = &[
    (Controller, 0.9, 1.0),
    (Pace, 0.82, 0.7),
    (Sound, 0.76, 0.7),
    (Couch, 0.5, 0.4),
    (Pause, 0.4, 0.4),
];
const SPORTS: Votes = &[
    (Couch, 0.72, 0.8),
    (Controller, 0.88, 0.9),
    (Social, 0.6, 0.5),
    (Sound, 0.5, 0.4),
    (Pace, 0.65, 0.4),
];
const FIGHTING: Votes = &[
    (Pace, 0.92, 1.0),
    (Couch, 0.78, 0.9),
    (Controller, 0.92, 0.9),
    (Social, 0.7, 0.6),
    (Sound, 0.66, 0.6),
    (Pause, 0.3, 0.5),
];
const LOCAL: Votes = &[
    (Couch, 1.0, 1.7),
    (Online, 0.08, 1.0),
    (Social, 0.82, 1.0),
    (Controller, 0.8, 0.7),
    (Pause, 0.5, 0.4),
];
const PARTY: Votes = &[
    (Couch, 0.95, 1.2),
    (Social, 0.9, 1.0),
    (Pace, 0.6, 0.4),
    (Brain, 0.25, 0.4),
    (Pause, 0.5, 0.3),
    (Scare, 0.0, 0.4),
];
const MULTIPLAYER: Votes = &[(Social, 0.66, 0.9), (Online, 0.45, 0.6), (Pause, 0.4, 0.5)];
const COOP: Votes = &[(Social, 0.72, 0.8), (Online, 0.5, 0.4), (Couch, 0.3, 0.3)];
const ONLINE_COOP: Votes = &[
    (Online, 0.82, 1.1),
    (Social, 0.86, 0.9),
    (Pause, 0.28, 0.8),
    (Couch, 0.1, 0.5),
];
const SINGLEPLAYER: Votes = &[
    (Online, 0.05, 1.2),
    (Social, 0.08, 1.0),
    (Pause, 0.74, 0.6),
    (Couch, 0.08, 0.6),
];
const DARK: Votes = &[(Scare, 0.45, 0.45), (Chill, 0.25, 0.4)];
const ZOMBIES: Votes = &[(Scare, 0.55, 0.6), (Pace, 0.65, 0.3)];
const GORE: Votes = &[(Scare, 0.35, 0.35), (Chill, 0.2, 0.4)];
const GENTLE: Votes = &[(Scare, 0.0, 0.9), (Chill, 0.7, 0.6)];
const FAMILY: Votes = &[(Scare, 0.0, 0.9), (Chill, 0.65, 0.5), (Couch, 0.55, 0.4)];
const CONTROLLER_TAG: Votes = &[(Controller, 1.0, 1.6)];
const MOUSE_ONLY: Votes = &[(Controller, 0.0, 1.6)];
const DIFFICULT: Votes = &[(Chill, 0.15, 0.8), (Pace, 0.6, 0.4), (Pause, 0.4, 0.5)];
const BOARD: Votes = &[
    (Pause, 0.95, 1.1),
    (Pace, 0.08, 1.0),
    (Brain, 0.8, 0.9),
    (Sound, 0.18, 1.0),
    (Chill, 0.55, 0.5),
    (Scare, 0.0, 0.4),
];
const REAL_TIME: Votes = &[
    (Pause, 0.5, 0.6),
    (Brain, 0.85, 0.9),
    (Pace, 0.7, 0.8),
    (Sound, 0.35, 0.6),
    (Controller, 0.15, 1.0),
];

/// What a store tag says about a game. Tag names are lower-cased by the caller.
/// Tags are the community's own words for a game, so a vote here is worth more
/// than a guess from its genre.
fn tag_votes(tag: &str) -> Votes {
    match tag {
        "turn-based"
        | "turn-based strategy"
        | "turn-based tactics"
        | "tactical rpg"
        | "strategy rpg"
        | "grid-based movement" => TURN_BASED,
        "turn-based combat" => &[(Pause, 0.88, 0.7), (Sound, 0.3, 0.5)],
        "strategy"
        | "grand strategy"
        | "4x"
        | "city builder"
        | "management"
        | "resource management"
        | "base building"
        | "colony sim"
        | "automation"
        | "economy"
        | "tower defense"
        | "real-time with pause"
        | "building"
        | "trading"
        | "diplomacy"
        | "political sim"
        | "wargame"
        | "capitalism"
        | "god game"
        | "time management"
        | "inventory management"
        | "shop keeper" => SLOW_STRATEGY,
        "rts" | "real time tactics" | "action rts" => REAL_TIME,
        "idler" | "incremental" | "clicker" | "match 3" | "solitaire" | "word game" | "trivia"
        | "typing" | "minigames" | "falling blocks" | "hidden object" | "mahjong" => IDLE,
        "board game" | "tabletop" | "chess" | "card game" | "card battler"
        | "trading card game" | "deckbuilding" | "dice" | "poker" | "abstract" => BOARD,
        "puzzle" | "logic" | "escape room" | "hacking" | "programming" | "sokoban" => PUZZLE,
        "visual novel"
        | "text-based"
        | "interactive fiction"
        | "dating sim"
        | "otome"
        | "choose your own adventure"
        | "dialogue heavy" => TEXT_DRIVEN,
        "relaxing" | "cozy" | "wholesome" | "farming sim" | "farming" | "life sim" | "cooking"
        | "cleaning" | "fishing" | "decorating" | "hobby sim" | "agriculture" | "organizing"
        | "mining" | "animals" | "cats" | "dogs" | "capybaras" => COZY,
        "cute" | "colorful" | "casual" | "cartoony" | "cartoon" | "hand-drawn" => GENTLE,
        "family friendly" => FAMILY,
        "horror"
        | "survival horror"
        | "psychological horror"
        | "jump scare"
        | "lovecraftian"
        | "gothic"
        | "werewolves"
        | "vampires"
        | "outbreak sim"
        | "cult" => HORROR,
        "thriller" | "supernatural" | "demons" | "psychological" => {
            &[(Scare, 0.5, 0.5), (Chill, 0.25, 0.4)]
        }
        "dark" | "dark fantasy" | "dystopian" => DARK,
        "zombies" => ZOMBIES,
        "gore" | "violent" => GORE,
        "rhythm" => RHYTHM,
        "music" | "music-based procedural generation" => MUSIC_FORWARD,
        "great soundtrack" | "soundtrack" | "electronic music" | "rock music" | "8-bit music"
        | "instrumental music" => SOUNDTRACK,
        "atmospheric" | "immersive" | "epic" | "dynamic narration" => ATMOSPHERIC,
        "cinematic" => CINEMATIC,
        "fps"
        | "shooter"
        | "first-person"
        | "third-person shooter"
        | "looter shooter"
        | "on-rails shooter"
        | "bullet time"
        | "military"
        | "sniper"
        | "gun customization" => SHOOTER,
        "tactical" | "tactical shooter" | "extraction shooter" | "hero shooter"
        | "arena shooter" => TACTICAL_SHOOTER,
        "boomer shooter" => &[(Sound, 0.6, 0.6), (Pace, 0.97, 1.0), (Pause, 0.5, 0.4)],
        "stealth" | "assassins" | "heist" | "espionage" | "ninja" => STEALTH,
        "pvp" | "competitive" | "esports" | "team-based" | "battle royale" | "class-based"
        | "moba" => COMPETITIVE,
        "massively multiplayer" | "mmorpg" => MASSIVELY,
        "souls-like" => SOULSLIKE,
        "fast-paced"
        | "bullet hell"
        | "shoot 'em up"
        | "twin stick shooter"
        | "musou"
        | "hack and slash"
        | "spectacle fighter"
        | "character action game"
        | "parkour"
        | "beat 'em up"
        | "top-down shooter"
        | "bullet heaven"
        | "action roguelike" => FAST,
        "arcade" | "score attack" | "time attack" | "runner" | "pinball" => ARCADE,
        "roguelike"
        | "roguelite"
        | "roguelike deckbuilder"
        | "traditional roguelike"
        | "perma death"
        | "procedural generation"
        | "mystery dungeon" => ROGUELIKE,
        "platformer"
        | "2d platformer"
        | "3d platformer"
        | "precision platformer"
        | "puzzle platformer"
        | "metroidvania"
        | "side scroller"
        | "collectathon" => PLATFORMER,
        "story rich" | "narrative" | "choices matter" | "multiple endings" | "emotional"
        | "romance" | "lore-rich" | "episodic" | "mystery" | "detective" | "investigation"
        | "noir" | "philosophical" | "conspiracy" | "silent protagonist" | "based on a novel" => {
            STORY
        }
        "walking simulator" => WALKING,
        "point & click" => ADVENTURE_CLICK,
        "open world" | "action-adventure" | "third person" => OPEN_WORLD,
        "rpg" | "action rpg" | "crpg" | "party-based rpg" | "dungeon crawler" => RPG,
        "jrpg" | "anime" => JRPG,
        "survival" | "open world survival craft" | "crafting" | "sandbox" | "post-apocalyptic" => {
            SURVIVAL
        }
        "exploration" | "nature" | "beautiful" | "underwater" | "space sim" | "flight" => EXPLORE,
        "racing" | "driving" | "automobile sim" | "combat racing" | "offroad" | "motorbike"
        | "vehicular combat" | "transportation" | "trains" => DRIVING,
        "sports"
        | "football (soccer)"
        | "basketball"
        | "golf"
        | "tennis"
        | "hockey"
        | "skiing"
        | "snowboarding"
        | "baseball"
        | "football (american)"
        | "bowling"
        | "mini golf"
        | "boxing"
        | "wrestling"
        | "cricket"
        | "rugby"
        | "volleyball" => SPORTS,
        "fighting" | "2d fighter" | "3d fighter" => FIGHTING,
        "local multiplayer" | "local co-op" | "split screen" | "4 player local" => LOCAL,
        "party game" | "party" | "social deduction" => PARTY,
        "multiplayer" => MULTIPLAYER,
        "co-op" | "co-op campaign" | "pve" => COOP,
        "online co-op" | "asynchronous multiplayer" => ONLINE_COOP,
        "singleplayer" => SINGLEPLAYER,
        "controller" => CONTROLLER_TAG,
        "mouse only" => MOUSE_ONLY,
        "difficult" | "boss rush" => DIFFICULT,
        _ => &[],
    }
}

/// What a store category (the feature checklist on a store page) says. These are
/// the developer's own claims, so the access features carry real weight.
fn category_votes(name: &str) -> Votes {
    match name {
        "save anytime" => &[(Pause, 0.97, 1.7)],
        "playable without timed input" => &[(Pause, 0.9, 1.0), (Pace, 0.15, 0.7)],
        "steam turn notifications" => &[(Pause, 0.92, 0.9), (Pace, 0.1, 0.6)],
        "captions available" => &[(Sound, 0.2, 0.55)],
        "subtitle options" => &[(Sound, 0.3, 0.3)],
        "surround sound" => &[(Sound, 0.8, 0.7)],
        "stereo sound" => &[(Sound, 0.56, 0.25)],
        "full controller support" => &[(Controller, 1.0, 2.0)],
        "partial controller support" => &[(Controller, 0.55, 1.3)],
        "online pvp" => &[
            (Online, 0.96, 1.6),
            (Social, 0.85, 1.0),
            (Pause, 0.1, 1.1),
            (Sound, 0.72, 0.6),
        ],
        "online co-op" => &[(Online, 0.76, 0.9), (Social, 0.82, 0.8), (Pause, 0.34, 0.6)],
        "pvp" => &[(Social, 0.76, 0.7), (Pause, 0.2, 0.7)],
        "mmo" => &[(Online, 1.0, 2.0), (Pause, 0.1, 1.5)],
        "co-op" => &[(Social, 0.7, 0.6)],
        "multi-player" => &[(Social, 0.6, 0.5)],
        "single-player" => &[
            (Social, 0.1, 0.8),
            (Online, 0.1, 0.9),
            (Pause, 0.7, 0.5),
            (Couch, 0.1, 0.4),
        ],
        "shared/split screen" => &[(Couch, 0.95, 1.4), (Controller, 0.75, 0.7)],
        "shared/split screen co-op" | "shared/split screen pvp" => {
            &[(Couch, 1.0, 1.6), (Online, 0.1, 0.8), (Social, 0.85, 0.8)]
        }
        "remote play together" => &[(Couch, 0.55, 0.4)],
        "cross-platform multiplayer" => &[(Online, 0.8, 0.4)],
        _ => &[],
    }
}

/// What a store genre says, at low weight: a genre is the broadest label a game
/// carries, so it breaks ties rather than deciding anything.
fn genre_votes(genre: &str) -> Votes {
    match genre {
        "casual" => &[(Sound, 0.28, 0.4), (Pause, 0.85, 0.5)],
        "strategy" => &[
            (Pause, 0.85, 0.6),
            (Sound, 0.22, 0.6),
            (Controller, 0.35, 0.4),
        ],
        "simulation" => &[(Pause, 0.75, 0.4), (Sound, 0.4, 0.3)],
        "rpg" => &[(Story, 0.75, 0.4), (Pause, 0.65, 0.3), (Sound, 0.56, 0.3)],
        "racing" => &[(Sound, 0.76, 0.6), (Controller, 0.9, 0.7)],
        "sports" => &[(Controller, 0.85, 0.6), (Couch, 0.65, 0.5)],
        "massively multiplayer" => &[(Online, 1.0, 1.4), (Pause, 0.1, 1.0)],
        "action" => &[(Sound, 0.6, 0.3)],
        "adventure" => &[(Sound, 0.58, 0.3), (Story, 0.7, 0.3)],
        _ => &[],
    }
}

fn cast_all(ballot: &mut Ballot, votes: Votes, label: &str, scale: f64) {
    for (axis, value, weight) in votes {
        ballot.cast(*axis, *value as f64, *weight as f64 * scale, label);
    }
}

/// Collects every piece of evidence a game carries into one ballot: tags first
/// (ranked by how many players voted for them), then the developer's feature
/// checklist, voice acting, genre, and the Steam Deck review.
pub fn collect(meta: &Meta) -> Ballot {
    let mut ballot = Ballot::default();

    for (rank, tag) in meta.tags.iter().take(20).enumerate() {
        let scale = 1.0 / (1.0 + 0.07 * rank as f64);
        cast_all(
            &mut ballot,
            tag_votes(&tag.to_ascii_lowercase()),
            tag,
            scale,
        );
    }

    for category in &meta.categories {
        cast_all(
            &mut ballot,
            category_votes(&category.to_ascii_lowercase()),
            category,
            1.0,
        );
    }

    for genre in &meta.genres {
        cast_all(
            &mut ballot,
            genre_votes(&genre.to_ascii_lowercase()),
            genre,
            1.0,
        );
    }

    if meta.audio_langs > 0 {
        if meta.audio_full > 0 {
            ballot.cast(Axis::Sound, 0.72, 0.65, "voice acting");
        } else {
            ballot.cast(Axis::Sound, 0.2, 0.75, "no voice acting");
        }
    }

    match meta.deck_input.as_deref() {
        Some("gamepad") => ballot.cast(Axis::Controller, 0.95, 1.0, "built for a gamepad"),
        Some("keyboardmouse") => ballot.cast(Axis::Controller, 0.12, 1.1, "wants a keyboard"),
        _ => {}
    }
    match meta.deck {
        3 => ballot.cast(Axis::Controller, 0.92, 0.7, "Deck verified"),
        2 => ballot.cast(Axis::Controller, 0.62, 0.3, "Deck playable"),
        1 => ballot.cast(Axis::Controller, 0.25, 0.4, "not Deck-ready"),
        _ => {}
    }
    match meta.needs_online {
        Some(true) => ballot.cast(Axis::Online, 0.92, 1.3, "needs a connection to play"),
        Some(false) if meta.categories.iter().any(|c| c == "Single-player") => {
            ballot.cast(Axis::Online, 0.05, 0.6, "works offline")
        }
        _ => {}
    }
    ballot
}

/// Voiced stories lean on the ear more than voiced anything else, so once the
/// story axis is known it gets a second say on sound.
pub fn voiced_story_vote(ballot: &mut Ballot, meta: &Meta, story: f64) {
    if meta.audio_full > 0 && story > 0.6 {
        ballot.cast(Axis::Sound, 0.85, 0.55 * story, "voiced story");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(tags: &[&str], categories: &[&str]) -> Meta {
        Meta {
            tags: tags.iter().map(|s| s.to_string()).collect(),
            categories: categories.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn turn_based_games_do_not_need_sound() {
        let b = collect(&meta(
            &["Turn-Based Strategy", "Strategy"],
            &["Single-player"],
        ));
        let sound = b.settle(Axis::Sound, 0.5, 0.5);
        assert!(sound.value < 0.25, "got {}", sound.value);
        assert!(sound.confidence > 0.6);
        assert!(sound.because.iter().any(|s| s.contains("Turn-Based")));
    }

    #[test]
    fn horror_and_rhythm_lean_on_sound() {
        let horror = collect(&meta(&["Horror", "Atmospheric"], &[])).settle(Axis::Sound, 0.5, 0.5);
        let rhythm = collect(&meta(&["Rhythm", "Music"], &[])).settle(Axis::Sound, 0.5, 0.5);
        assert!(horror.value > 0.75, "got {}", horror.value);
        assert!(rhythm.value > 0.85, "got {}", rhythm.value);
    }

    #[test]
    fn no_voice_acting_pulls_sound_down() {
        let mut m = meta(&["Story Rich"], &[]);
        m.audio_langs = 6;
        m.audio_full = 0;
        let silent = collect(&m).settle(Axis::Sound, 0.5, 0.5);
        m.audio_full = 4;
        let voiced = collect(&m).settle(Axis::Sound, 0.5, 0.5);
        assert!(silent.value < voiced.value - 0.15);
    }

    #[test]
    fn save_anytime_makes_a_game_interruptible() {
        let with = collect(&meta(&["Action"], &["Save Anytime"])).settle(Axis::Pause, 0.5, 0.5);
        let without = collect(&meta(&["Action"], &[])).settle(Axis::Pause, 0.5, 0.5);
        assert!(with.value > without.value + 0.2);
    }

    #[test]
    fn online_only_games_are_not_offline_safe() {
        let b = collect(&meta(
            &["Multiplayer", "PvP"],
            &["Online PvP", "Multi-player"],
        ));
        assert!(b.settle(Axis::Online, 0.3, 0.3).value > 0.75);
        let solo = collect(&meta(&["Singleplayer"], &["Single-player"]));
        assert!(solo.settle(Axis::Online, 0.3, 0.5).value < 0.15);
    }

    #[test]
    fn controller_support_is_read_from_the_checklist() {
        let full = collect(&meta(&["Platformer"], &["Full controller support"]));
        let none = collect(&meta(&["Platformer"], &[]));
        assert!(full.settle(Axis::Controller, 0.5, 0.3).value > 0.8);
        assert!(
            none.settle(Axis::Controller, 0.5, 0.5).value
                < full.settle(Axis::Controller, 0.5, 0.5).value
        );
    }

    #[test]
    fn assume_pulls_unknown_games_toward_the_fallback() {
        let t = Trait {
            value: 0.05,
            confidence: 0.1,
            ..Trait::guess(0.05)
        };
        assert!(t.assume(0.6) > 0.4);
        let sure = Trait {
            value: 0.05,
            confidence: 0.99,
            ..Trait::guess(0.05)
        };
        assert!(sure.assume(0.6) < 0.1);
    }

    #[test]
    fn deck_verified_is_category_three_and_unsupported_is_one() {
        let mut m = meta(&["Platformer"], &[]);
        m.deck = 3;
        let verified = collect(&m).settle(Axis::Controller, 0.5, 0.3);
        m.deck = 1;
        let unsupported = collect(&m).settle(Axis::Controller, 0.5, 0.3);
        m.deck = 2;
        let playable = collect(&m).settle(Axis::Controller, 0.5, 0.3);
        assert!(verified.value > playable.value);
        assert!(playable.value > unsupported.value);
        assert!(verified.high.iter().any(|l| l == "Deck verified"));
    }
}
