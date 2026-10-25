use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS games (
  appid        INTEGER PRIMARY KEY,
  name         TEXT    NOT NULL DEFAULT '',
  playtime_min INTEGER NOT NULL DEFAULT 0,
  playtime_2w  INTEGER NOT NULL DEFAULT 0,
  last_played  INTEGER NOT NULL DEFAULT 0,
  installed    INTEGER NOT NULL DEFAULT 0,
  size_bytes   INTEGER NOT NULL DEFAULT 0,
  source       TEXT    NOT NULL DEFAULT 'local',
  first_seen   INTEGER NOT NULL DEFAULT 0,
  updated_at   INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS meta (
  appid              INTEGER PRIMARY KEY,
  name               TEXT,
  kind               TEXT,
  short_desc         TEXT,
  genres             TEXT NOT NULL DEFAULT '[]',
  categories         TEXT NOT NULL DEFAULT '[]',
  developers         TEXT NOT NULL DEFAULT '[]',
  release_date       TEXT,
  release_year       INTEGER NOT NULL DEFAULT 0,
  metacritic         INTEGER NOT NULL DEFAULT 0,
  recommendations    INTEGER NOT NULL DEFAULT 0,
  is_free            INTEGER NOT NULL DEFAULT 0,
  price_initial      INTEGER NOT NULL DEFAULT 0,
  currency           TEXT,
  achievements_total INTEGER NOT NULL DEFAULT 0,
  fetched_at         INTEGER NOT NULL DEFAULT 0,
  ok                 INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS prefs (
  appid           INTEGER PRIMARY KEY,
  amnesty         INTEGER NOT NULL DEFAULT 0,
  pinned          INTEGER NOT NULL DEFAULT 0,
  session_minutes INTEGER,
  elo             REAL    NOT NULL DEFAULT 1500,
  duels           INTEGER NOT NULL DEFAULT 0,
  note            TEXT,
  updated_at      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS events (
  id       INTEGER PRIMARY KEY AUTOINCREMENT,
  ts       INTEGER NOT NULL,
  appid    INTEGER NOT NULL,
  kind     TEXT    NOT NULL,
  budget   INTEGER,
  mood     TEXT,
  appetite TEXT,
  seconds  INTEGER,
  note     TEXT
);
CREATE INDEX IF NOT EXISTS idx_events_app  ON events(appid, kind, ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_kind ON events(kind, ts DESC);

CREATE TABLE IF NOT EXISTS sessions (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  appid           INTEGER NOT NULL,
  started_at      INTEGER NOT NULL,
  ended_at        INTEGER,
  budget          INTEGER,
  mood            TEXT,
  appetite        TEXT,
  verdict         TEXT,
  first_impression TEXT,
  hand_id         TEXT
);
CREATE INDEX IF NOT EXISTS idx_sessions_open ON sessions(ended_at, started_at DESC);

CREATE TABLE IF NOT EXISTS hands (
  id            TEXT PRIMARY KEY,
  ts            INTEGER NOT NULL,
  budget        INTEGER,
  mood          TEXT,
  appetite      TEXT,
  cards         TEXT NOT NULL,
  passes        INTEGER NOT NULL DEFAULT 0,
  rerolls       INTEGER NOT NULL DEFAULT 0,
  resolved      INTEGER,
  reroll_of     TEXT
);

CREATE TABLE IF NOT EXISTS achievements (
  appid      INTEGER PRIMARY KEY,
  unlocked   INTEGER NOT NULL DEFAULT 0,
  total      INTEGER NOT NULL DEFAULT 0,
  fetched_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS settings (k TEXT PRIMARY KEY, v TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS taste (
  dim    TEXT PRIMARY KEY,
  weight REAL    NOT NULL DEFAULT 0,
  n      INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS overrides (
  appid      INTEGER NOT NULL,
  trait      TEXT    NOT NULL,
  value      REAL    NOT NULL,
  updated_at INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (appid, trait)
);
"#;

/// Columns added after the first release. Each is applied once, in order, to
/// databases created by an earlier version.
const COLUMNS: &[(&str, &str, &str)] = &[
    ("meta", "tags", "TEXT NOT NULL DEFAULT '[]'"),
    ("meta", "l_genres", "TEXT NOT NULL DEFAULT '[]'"),
    ("meta", "l_categories", "TEXT NOT NULL DEFAULT '[]'"),
    ("meta", "l_metacritic", "INTEGER NOT NULL DEFAULT 0"),
    ("meta", "l_release_year", "INTEGER NOT NULL DEFAULT 0"),
    ("meta", "audio_langs", "INTEGER NOT NULL DEFAULT -1"),
    ("meta", "audio_full", "INTEGER NOT NULL DEFAULT -1"),
    ("meta", "subtitles", "INTEGER NOT NULL DEFAULT -1"),
    ("meta", "deck", "INTEGER NOT NULL DEFAULT 0"),
    ("meta", "deck_input", "TEXT"),
    ("meta", "needs_online", "INTEGER NOT NULL DEFAULT -1"),
    ("meta", "review_pct", "INTEGER NOT NULL DEFAULT 0"),
    ("meta", "local_at", "INTEGER NOT NULL DEFAULT 0"),
    ("hands", "conditions", "TEXT NOT NULL DEFAULT '[]'"),
    ("hands", "viable", "INTEGER NOT NULL DEFAULT 0"),
    ("hands", "library", "INTEGER NOT NULL DEFAULT 0"),
    ("hands", "relaxed", "TEXT"),
    ("sessions", "conditions", "TEXT NOT NULL DEFAULT '[]'"),
    ("sessions", "bail_reason", "TEXT"),
];

fn migrate(conn: &Connection) -> Result<()> {
    for (table, column, ddl) in COLUMNS {
        let present: bool = conn
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == *column);
        if !present {
            conn.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"),
                [],
            )?;
        }
    }
    Ok(())
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        Ok(Db {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs `f` under the connection mutex. Never `await` inside `f`.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let guard = self.conn.lock().unwrap();
        f(&guard)
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.with(|c| {
            Ok(
                c.query_row("SELECT v FROM settings WHERE k=?1", params![key], |r| {
                    r.get::<_, String>(0)
                })
                .optional()?,
            )
        })
        .ok()
        .flatten()
    }

    pub fn setting_or(&self, key: &str, default: &str) -> String {
        self.setting(key).unwrap_or_else(|| default.to_string())
    }

    pub fn setting_i64(&self, key: &str, default: i64) -> i64 {
        self.setting(key)
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(default)
    }

    pub fn setting_bool(&self, key: &str, default: bool) -> bool {
        match self.setting(key).as_deref() {
            Some("1") | Some("true") => true,
            Some("0") | Some("false") => false,
            _ => default,
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings(k,v) VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
                params![key, value],
            )?;
            Ok(())
        })
    }

    pub fn log_note(&self, appid: i64, kind: &str, note: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO events(ts,appid,kind,note) VALUES(?1,?2,?3,?4)",
                params![now(), appid, kind, note],
            )?;
            Ok(())
        })
    }

    pub fn log_event(
        &self,
        appid: i64,
        kind: &str,
        budget: Option<i64>,
        mood: Option<&str>,
        appetite: Option<&str>,
        seconds: Option<i64>,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO events(ts,appid,kind,budget,mood,appetite,seconds)
                 VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![now(), appid, kind, budget, mood, appetite, seconds],
            )?;
            Ok(())
        })
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Calendar year of a unix timestamp, exact across leap years.
pub fn year_of(ts: i64) -> i64 {
    let z = ts.div_euclid(86400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    yoe + era * 400 + i64::from(month <= 2)
}

#[cfg(test)]
mod tests {
    use super::year_of;

    #[test]
    fn year_of_is_exact_at_boundaries() {
        assert_eq!(year_of(0), 1970);
        assert_eq!(year_of(946_684_799), 1999);
        assert_eq!(year_of(946_684_800), 2000);
        assert_eq!(year_of(1_735_689_599), 2024);
        assert_eq!(year_of(1_735_689_600), 2025);
    }
}
