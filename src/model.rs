use serde::{Deserialize, Serialize};

pub const MOODS: &[(&str, &str, &str)] = &[
    ("fried", "Fried", "brain off, hands on"),
    ("mellow", "Mellow", "unwind, no pressure"),
    ("sharp", "Sharp", "give me something to chew"),
    ("restless", "Restless", "I need stimulation now"),
    ("social", "Social", "someone else is here"),
];

pub const APPETITES: &[(&str, &str, &str)] = &[
    ("comfort", "Comfort food", "something I already love"),
    (
        "unfinished",
        "Unfinished business",
        "I owe one a fair trial",
    ),
    ("stranger", "A stranger", "something I've never opened"),
    ("surprise", "Dealer's choice", "read me"),
];

pub const BUDGETS: &[(i64, &str)] = &[
    (20, "20 minutes"),
    (45, "45 minutes"),
    (90, "An hour and a half"),
    (180, "The whole evening"),
    (0, "No idea"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub appid: i64,
    pub name: String,
    pub playtime_min: i64,
    pub playtime_2w: i64,
    pub last_played: i64,
    pub installed: bool,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    pub kind: String,
    pub short_desc: String,
    pub genres: Vec<String>,
    pub categories: Vec<String>,
    pub developers: Vec<String>,
    pub release_year: i64,
    pub metacritic: i64,
    pub recommendations: i64,
    pub price_initial: i64,
    pub currency: String,
    pub achievements_total: i64,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefs {
    pub amnesty: bool,
    pub pinned: bool,
    pub session_override: Option<i64>,
    pub elo: f64,
    pub duels: i64,
}

/// Everything the engine needs about one game, assembled once per deal.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub game: Game,
    pub meta: Meta,
    #[serde(skip)]
    pub prefs: Prefs,
    pub shape: crate::engine::shape::Shape,
    pub history: History,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct History {
    pub passes: i64,
    pub last_pass: i64,
    pub launches: i64,
    pub bounces: i64,
    pub loved: i64,
    pub last_dealt: i64,
    pub last_verdict: String,
    pub ach_unlocked: i64,
    pub ach_total: i64,
    pub observed_session_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reason {
    pub weight: f64,
    pub text: String,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    SureThing,
    Unfinished,
    Stranger,
    Wildcard,
    Rebound,
}

impl Slot {
    pub fn label(&self) -> &'static str {
        match self {
            Slot::SureThing => "The sure thing",
            Slot::Unfinished => "Unfinished business",
            Slot::Stranger => "The stranger",
            Slot::Wildcard => "The wildcard",
            Slot::Rebound => "The rebound",
        }
    }

    /// The role description shown under a card. The stranger slot also accepts
    /// games opened once for a few minutes, so its blurb has to tell the truth
    /// about which of the two it is.
    pub fn blurb(&self, playtime_min: i64) -> &'static str {
        match self {
            Slot::SureThing => "You know exactly what this is. It will not waste your evening.",
            Slot::Unfinished => "You started it and walked away. It never got a fair trial.",
            Slot::Stranger if playtime_min == 0 => {
                "Bought, filed, forgotten. Tonight it gets one shot."
            }
            Slot::Stranger => "You looked at it once for a few minutes. That is not a trial.",
            Slot::Wildcard => "No argument for it except that it might be the one.",
            Slot::Rebound => "That one didn't take. Here's the next.",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub appid: i64,
    pub name: String,
    pub slot: Slot,
    pub slot_label: String,
    pub slot_blurb: String,
    pub score: f64,
    pub verdict: String,
    pub caution: Option<String>,
    pub reasons: Vec<Reason>,
    pub session_label: String,
    pub session_minutes: i64,
    pub playtime_min: i64,
    pub last_played: i64,
    pub installed: bool,
    pub genres: Vec<String>,
    pub metacritic: i64,
    pub short_desc: String,
    pub achievements_total: i64,
    pub release_year: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Hand {
    pub id: String,
    pub cards: Vec<Card>,
    pub passes_used: i64,
    pub passes_allowed: i64,
    pub rerolls: i64,
    pub budget: i64,
    pub mood: String,
    pub appetite: String,
    pub locked: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DealRequest {
    #[serde(default)]
    pub budget: i64,
    #[serde(default)]
    pub mood: String,
    #[serde(default)]
    pub appetite: String,
    #[serde(default)]
    pub exclude: Vec<i64>,
    #[serde(default)]
    pub reroll_of: Option<String>,
}
