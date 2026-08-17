use crate::db::{now, Db};
use anyhow::{anyhow, Result};
use rusqlite::params;
use serde::Deserialize;
use serde_json::Value;
use std::time::Duration;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("tonight/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(25))
        .build()
        .expect("http client")
}

#[derive(Debug, Deserialize)]
struct OwnedResponse {
    response: OwnedBody,
}

#[derive(Debug, Deserialize, Default)]
struct OwnedBody {
    #[serde(default)]
    game_count: i64,
    #[serde(default)]
    games: Vec<OwnedGame>,
}

#[derive(Debug, Deserialize)]
pub struct OwnedGame {
    pub appid: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub playtime_forever: i64,
    #[serde(default)]
    pub playtime_2weeks: i64,
    #[serde(default)]
    pub rtime_last_played: i64,
}

/// Pulls the authoritative owned-games list, including titles never installed
/// on this machine. Requires a Steam Web API key.
pub async fn fetch_owned(
    http: &reqwest::Client,
    key: &str,
    steamid: &str,
) -> Result<Vec<OwnedGame>> {
    let url = format!(
        "https://api.steampowered.com/IPlayerService/GetOwnedGames/v1/?key={key}&steamid={steamid}\
         &include_appinfo=1&include_played_free_games=1&skip_unvetted_apps=0&format=json"
    );
    let res = http.get(url).send().await?;
    if res.status() == 401 || res.status() == 403 {
        return Err(anyhow!(
            "Steam rejected the API key or the profile is private (HTTP {})",
            res.status()
        ));
    }
    if !res.status().is_success() {
        return Err(anyhow!("Steam Web API returned HTTP {}", res.status()));
    }
    let body: OwnedResponse = res.json().await?;
    if body.response.games.is_empty() && body.response.game_count == 0 {
        return Err(anyhow!(
            "Steam returned an empty library — set Game details to Public in your Steam privacy settings"
        ));
    }
    Ok(body.response.games)
}

pub fn ingest_owned(db: &Db, games: &[OwnedGame]) -> Result<usize> {
    let ts = now();
    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        {
            let mut up = tx.prepare(
                "INSERT INTO games(appid,name,playtime_min,playtime_2w,last_played,source,first_seen,updated_at)
                 VALUES(?1,?2,?3,?4,?5,'web',?6,?6)
                 ON CONFLICT(appid) DO UPDATE SET
                   name         = CASE WHEN ?2<>'' THEN ?2 ELSE games.name END,
                   playtime_min = MAX(games.playtime_min, ?3),
                   playtime_2w  = ?4,
                   last_played  = MAX(games.last_played, ?5),
                   source       = 'web',
                   updated_at   = ?6",
            )?;
            for g in games {
                up.execute(params![
                    g.appid,
                    g.name,
                    g.playtime_forever,
                    g.playtime_2weeks,
                    g.rtime_last_played,
                    ts
                ])?;
            }
        }
        tx.commit()?;
        Ok(games.len())
    })
}

#[derive(Debug, Default)]
pub struct AppDetails {
    pub name: String,
    pub kind: String,
    pub short_desc: String,
    pub genres: Vec<String>,
    pub categories: Vec<String>,
    pub developers: Vec<String>,
    pub release_date: String,
    pub release_year: i64,
    pub metacritic: i64,
    pub recommendations: i64,
    pub is_free: bool,
    pub price_initial: i64,
    pub currency: String,
    pub achievements_total: i64,
}

pub enum FetchOutcome {
    Got(Box<AppDetails>),
    NotAGame,
    RateLimited,
}

/// Queries the public storefront for genre/category/score metadata. No key needed.
pub async fn fetch_details(
    http: &reqwest::Client,
    appid: i64,
    cc: &str,
    lang: &str,
) -> Result<FetchOutcome> {
    let url =
        format!("https://store.steampowered.com/api/appdetails?appids={appid}&cc={cc}&l={lang}");
    let res = http.get(url).send().await?;
    if res.status() == 429 || res.status() == 403 {
        return Ok(FetchOutcome::RateLimited);
    }
    if !res.status().is_success() {
        return Err(anyhow!("appdetails HTTP {}", res.status()));
    }
    let v: Value = res.json().await?;
    let node = v.get(appid.to_string()).ok_or_else(|| anyhow!("no node"))?;
    if !node
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(FetchOutcome::NotAGame);
    }
    let d = node.get("data").ok_or_else(|| anyhow!("no data"))?;
    let strings = |key: &str, field: &str| -> Vec<String> {
        d.get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        x.get(field)
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or_else(|| x.as_str().map(str::to_string))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let release_date = d
        .pointer("/release_date/date")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let details = AppDetails {
        name: d
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        kind: d
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("game")
            .to_string(),
        short_desc: d
            .get("short_description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        genres: strings("genres", "description"),
        categories: strings("categories", "description"),
        developers: strings("developers", "__none__"),
        release_year: parse_year(&release_date),
        release_date,
        metacritic: d
            .pointer("/metacritic/score")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        recommendations: d
            .pointer("/recommendations/total")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        is_free: d.get("is_free").and_then(Value::as_bool).unwrap_or(false),
        price_initial: d
            .pointer("/price_overview/initial")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        currency: d
            .pointer("/price_overview/currency")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        achievements_total: d
            .pointer("/achievements/total")
            .and_then(Value::as_i64)
            .unwrap_or(0),
    };
    Ok(FetchOutcome::Got(Box::new(details)))
}

fn parse_year(s: &str) -> i64 {
    let mut best = 0;
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let w: String = bytes[i..i + 4].iter().collect();
        if w.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(y) = w.parse::<i64>() {
                if (1970..=2100).contains(&y) {
                    best = y;
                }
            }
        }
        i += 1;
    }
    best
}

pub fn store_details(db: &Db, appid: i64, d: &AppDetails) -> Result<()> {
    db.with(|c| {
        c.execute(
            "INSERT INTO meta(appid,name,kind,short_desc,genres,categories,developers,release_date,
                              release_year,metacritic,recommendations,is_free,price_initial,currency,
                              achievements_total,fetched_at,ok)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,1)
             ON CONFLICT(appid) DO UPDATE SET
               name=excluded.name, kind=excluded.kind, short_desc=excluded.short_desc,
               genres=excluded.genres, categories=excluded.categories, developers=excluded.developers,
               release_date=excluded.release_date, release_year=excluded.release_year,
               metacritic=excluded.metacritic, recommendations=excluded.recommendations,
               is_free=excluded.is_free, price_initial=excluded.price_initial,
               currency=excluded.currency, achievements_total=excluded.achievements_total,
               fetched_at=excluded.fetched_at, ok=1",
            params![
                appid,
                d.name,
                d.kind,
                d.short_desc,
                serde_json::to_string(&d.genres)?,
                serde_json::to_string(&d.categories)?,
                serde_json::to_string(&d.developers)?,
                d.release_date,
                d.release_year,
                d.metacritic,
                d.recommendations,
                d.is_free as i64,
                d.price_initial,
                d.currency,
                d.achievements_total,
                now()
            ],
        )?;
        if !d.name.is_empty() {
            c.execute(
                "UPDATE games SET name=?2 WHERE appid=?1 AND (name='' OR name IS NULL)",
                params![appid, d.name],
            )?;
        }
        Ok(())
    })
}

pub fn mark_unfetchable(db: &Db, appid: i64) -> Result<()> {
    db.with(|c| {
        c.execute(
            "INSERT INTO meta(appid,kind,fetched_at,ok) VALUES(?1,'unknown',?2,0)
             ON CONFLICT(appid) DO UPDATE SET fetched_at=?2, ok=0",
            params![appid, now()],
        )?;
        Ok(())
    })
}

/// Appids that still need storefront metadata, most-played first so the games
/// most likely to be dealt become useful earliest.
pub fn pending_enrichment(db: &Db, stale_after_days: i64) -> Result<Vec<i64>> {
    let cutoff = now() - stale_after_days * 86400;
    db.with(|c| {
        let mut st = c.prepare(
            "SELECT g.appid FROM games g
             LEFT JOIN meta m ON m.appid = g.appid
             WHERE m.appid IS NULL OR (m.ok=1 AND m.fetched_at < ?1)
             ORDER BY g.installed DESC, g.playtime_min DESC, g.appid",
        )?;
        let ids = st
            .query_map(params![cutoff], |r| r.get::<_, i64>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ids)
    })
}
