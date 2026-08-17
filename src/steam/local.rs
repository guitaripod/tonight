use super::appinfo;
use super::vdf::{self, Vdf};
use crate::db::{now, Db};
use anyhow::{Context, Result};
use rusqlite::params;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct LocalScan {
    pub steam_root: String,
    pub account_name: Option<String>,
    pub steamid64: Option<String>,
    pub libraries: Vec<String>,
    pub catalogue: usize,
    pub apps_seen: usize,
    pub installed: usize,
    pub with_playtime: usize,
    pub named: usize,
}

#[derive(Debug, Default, Clone)]
pub struct LocalApp {
    pub appid: i64,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub playtime_min: i64,
    pub last_played: i64,
    pub installed: bool,
    pub size_bytes: i64,
}

/// Reads everything the Steam client already knows, with no credentials at all:
/// owned/installed apps, on-disk size, minutes played and last-played stamps.
pub fn scan(root: &Path) -> Result<(LocalScan, Vec<LocalApp>)> {
    let mut report = LocalScan {
        steam_root: root.display().to_string(),
        ..Default::default()
    };
    let mut apps: HashMap<i64, LocalApp> = HashMap::new();

    if let Some((sid, account)) = primary_user(root) {
        report.steamid64 = Some(sid);
        report.account_name = account;
    }

    let catalogue = appinfo::parse(&root.join("appcache/appinfo.vdf")).unwrap_or_default();
    report.catalogue = catalogue.len();

    for lib in library_paths(root) {
        report.libraries.push(lib.display().to_string());
        for (appid, name, size) in manifests(&lib) {
            let e = apps.entry(appid).or_insert_with(|| LocalApp {
                appid,
                ..Default::default()
            });
            e.installed = true;
            e.size_bytes = size;
            if e.name.is_none() {
                e.name = name;
            }
        }
    }

    for dir in userdata_dirs(root) {
        let cfg = dir.join("config/localconfig.vdf");
        if cfg.is_file() {
            for (appid, mins, last) in playtimes(&cfg) {
                let e = apps.entry(appid).or_insert_with(|| LocalApp {
                    appid,
                    ..Default::default()
                });
                e.playtime_min = e.playtime_min.max(mins);
                e.last_played = e.last_played.max(last);
            }
        }
        for appid in cloud_app_dirs(&dir) {
            apps.entry(appid).or_insert_with(|| LocalApp {
                appid,
                ..Default::default()
            });
        }
    }

    for (appid, entry) in &catalogue {
        if let Some(a) = apps.get_mut(appid) {
            a.name = Some(entry.name.clone());
            a.kind = Some(entry.kind.clone());
        }
    }

    let mut out: Vec<LocalApp> = apps
        .into_values()
        .filter(is_plausible_game)
        .collect();
    out.sort_by_key(|a| a.appid);
    report.apps_seen = out.len();
    report.installed = out.iter().filter(|a| a.installed).count();
    report.with_playtime = out.iter().filter(|a| a.playtime_min > 0).count();
    report.named = out.iter().filter(|a| a.name.is_some()).count();
    Ok((report, out))
}

/// Filters out Steam's internal tools and runtimes that pollute the local config.
fn is_plausible_game(a: &LocalApp) -> bool {
    const NON_GAMES: &[i64] = &[
        7, 228980, 1070560, 1391110, 1493710, 1826330, 1887720, 2180100, 250820, 323910, 353370,
        365670, 1161040, 1628350, 1070550,
    ];
    if NON_GAMES.contains(&a.appid) {
        return false;
    }
    if let Some(k) = &a.kind {
        if matches!(
            k.as_str(),
            "dlc" | "music" | "video" | "tool" | "config" | "driver" | "beta"
        ) {
            return false;
        }
    }
    if let Some(n) = &a.name {
        let l = n.to_ascii_lowercase();
        if l.contains("steamworks common redistributables")
            || l.starts_with("proton ")
            || l.starts_with("steam linux runtime")
            || l.contains("steamvr")
        {
            return false;
        }
    }
    a.appid > 0
}

fn primary_user(root: &Path) -> Option<(String, Option<String>)> {
    let text = std::fs::read_to_string(root.join("config/loginusers.vdf")).ok()?;
    let v = vdf::parse(&text);
    let users = v.child("users")?.as_map()?;
    let mut best: Option<(String, Option<String>, i64)> = None;
    for (sid, node) in users {
        let ts = node.get_i64("timestamp").unwrap_or(0);
        let recent = node
            .get_str("mostrecent")
            .map(|s| s == "1")
            .unwrap_or(false);
        let rank = if recent { ts + (1 << 40) } else { ts };
        let name = node
            .get_str("personaname")
            .or_else(|| node.get_str("accountname"))
            .map(str::to_string);
        if best.as_ref().map(|b| rank > b.2).unwrap_or(true) {
            best = Some((sid.clone(), name, rank));
        }
    }
    best.map(|(a, b, _)| (a, b))
}

fn userdata_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(root.join("userdata")) {
        for e in rd.flatten() {
            if e.path().is_dir()
                && e.file_name()
                    .to_string_lossy()
                    .chars()
                    .all(|c| c.is_ascii_digit())
            {
                out.push(e.path());
            }
        }
    }
    out
}

fn cloud_app_dirs(userdata: &Path) -> Vec<i64> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(userdata) {
        for e in rd.flatten() {
            if let Ok(id) = e.file_name().to_string_lossy().parse::<i64>() {
                if id > 10 {
                    out.push(id);
                }
            }
        }
    }
    out
}

pub fn library_paths(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![root.join("steamapps")];
    let f = root.join("steamapps/libraryfolders.vdf");
    if let Ok(text) = std::fs::read_to_string(&f) {
        let v = vdf::parse(&text);
        if let Some(map) = v.child("libraryfolders").and_then(Vdf::as_map) {
            for node in map.values() {
                if let Some(p) = node.get_str("path") {
                    let sa = PathBuf::from(p).join("steamapps");
                    if sa.is_dir() && !out.contains(&sa) {
                        out.push(sa);
                    }
                }
            }
        }
    }
    out.retain(|p| p.is_dir());
    out.dedup();
    out
}

fn manifests(steamapps: &Path) -> Vec<(i64, Option<String>, i64)> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(steamapps) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let v = vdf::parse(&text);
        let Some(app) = v.child("appstate") else {
            continue;
        };
        let Some(appid) = app.get_i64("appid") else {
            continue;
        };
        out.push((
            appid,
            app.get_str("name").map(str::to_string),
            app.get_i64("sizeondisk").unwrap_or(0),
        ));
    }
    out
}

fn playtimes(localconfig: &Path) -> Vec<(i64, i64, i64)> {
    let Ok(text) = std::fs::read_to_string(localconfig) else {
        return vec![];
    };
    let v = vdf::parse(&text);
    let Some(apps) = v.path(&["userlocalconfigstore", "software", "valve", "steam", "apps"]) else {
        return vec![];
    };
    let Some(map) = apps.as_map() else {
        return vec![];
    };
    let mut out = vec![];
    for (k, node) in map {
        let Ok(appid) = k.parse::<i64>() else {
            continue;
        };
        let mins = node.get_i64("playtime").unwrap_or(0);
        let last = node.get_i64("lastplayed").unwrap_or(0);
        if last == 86400 {
            out.push((appid, mins, 0));
        } else {
            out.push((appid, mins, last));
        }
    }
    out
}

/// Merges a local scan into the library, preserving richer data already present
/// (a Web API name beats a manifest name; a larger playtime beats a stale one).
pub fn ingest(db: &Db, apps: &[LocalApp]) -> Result<usize> {
    let ts = now();
    db.with(|c| {
        let tx = c.unchecked_transaction()?;
        {
            let mut up = tx.prepare(
                "INSERT INTO games(appid,name,playtime_min,last_played,installed,size_bytes,source,first_seen,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,'local',?7,?7)
                 ON CONFLICT(appid) DO UPDATE SET
                   name         = CASE WHEN ?2<>'' THEN ?2 ELSE games.name END,
                   playtime_min = MAX(games.playtime_min, ?3),
                   last_played  = MAX(games.last_played, ?4),
                   installed    = ?5,
                   size_bytes   = MAX(games.size_bytes, ?6),
                   updated_at   = ?7",
            )?;
            let mut kind = tx.prepare(
                "INSERT INTO meta(appid,kind,fetched_at,ok) VALUES(?1,?2,0,1)
                 ON CONFLICT(appid) DO UPDATE SET kind=CASE WHEN meta.fetched_at=0 THEN ?2 ELSE meta.kind END",
            )?;
            for a in apps {
                if let Some(k) = a.kind.as_deref().filter(|k| !k.is_empty()) {
                    kind.execute(params![a.appid, k])?;
                }
                up.execute(params![
                    a.appid,
                    a.name.clone().unwrap_or_default(),
                    a.playtime_min,
                    a.last_played,
                    a.installed as i64,
                    a.size_bytes,
                    ts
                ])?;
            }
            let installed: Vec<String> = apps
                .iter()
                .filter(|a| a.installed)
                .map(|a| a.appid.to_string())
                .collect();
            tx.execute(
                &format!(
                    "UPDATE games SET installed=0 WHERE installed=1 AND appid NOT IN ({})",
                    if installed.is_empty() {
                        "0".into()
                    } else {
                        installed.join(",")
                    }
                ),
                [],
            )?;
        }
        tx.commit()?;
        Ok(apps.len())
    })
}

pub fn scan_and_ingest(db: &Db, root: &Path) -> Result<LocalScan> {
    let (report, apps) = scan(root).context("scanning Steam directory")?;
    ingest(db, &apps)?;
    if let Some(sid) = &report.steamid64 {
        if db.setting("steamid").is_none() {
            db.set_setting("steamid", sid)?;
        }
    }
    db.set_setting("last_local_scan", &now().to_string())?;
    Ok(report)
}
