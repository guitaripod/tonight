use crate::db::{now, Db};
use crate::steam::{images, local, web};
use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize, Default)]
pub struct SyncStatus {
    pub running: bool,
    pub phase: String,
    pub message: String,
    pub done: i64,
    pub total: i64,
    pub started_at: i64,
    pub finished_at: i64,
    pub error: Option<String>,
    pub library_size: i64,
    pub enriched: i64,
}

#[derive(Clone)]
pub struct Syncer {
    tx: watch::Sender<SyncStatus>,
    pub rx: watch::Receiver<SyncStatus>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

impl Default for Syncer {
    fn default() -> Self {
        Self::new()
    }
}

impl Syncer {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(SyncStatus::default());
        Syncer {
            tx,
            rx,
            busy: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn status(&self) -> SyncStatus {
        self.rx.borrow().clone()
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn emit(&self, mut f: impl FnMut(&mut SyncStatus)) {
        self.tx.send_modify(|s| f(s));
    }

    /// Kicks off the whole ingest pipeline in the background. Safe to call while
    /// the UI is open; the first two phases finish in well under a second.
    pub fn spawn(
        &self,
        db: Db,
        http: reqwest::Client,
        steam_root: Option<PathBuf>,
        img_dir: PathBuf,
        full: bool,
    ) -> bool {
        if self.busy.swap(true, Ordering::Relaxed) {
            return false;
        }
        self.cancel.store(false, Ordering::Relaxed);
        let me = self.clone();
        tokio::spawn(async move {
            me.emit(|s| {
                *s = SyncStatus {
                    running: true,
                    phase: "start".into(),
                    message: "Reading your Steam client…".into(),
                    started_at: now(),
                    ..Default::default()
                }
            });
            let result = me.run(&db, &http, steam_root, &img_dir, full).await;
            me.emit(move |s| {
                s.running = false;
                s.finished_at = now();
                s.phase = "done".into();
                if s.error.is_none() {
                    s.message = "Library is up to date.".into();
                }
            });
            if let Err(e) = result {
                me.emit(|s| {
                    s.error = Some(e.to_string());
                    s.message = format!("Stopped: {e}");
                });
            }
            me.busy.store(false, Ordering::Relaxed);
        });
        true
    }

    async fn run(
        &self,
        db: &Db,
        http: &reqwest::Client,
        steam_root: Option<PathBuf>,
        img_dir: &Path,
        full: bool,
    ) -> Result<()> {
        if let Some(root) = steam_root {
            self.emit(|s| {
                s.phase = "local".into();
                s.message = "Reading local Steam files…".into();
            });
            match local::scan_and_ingest(db, &root) {
                Ok(report) => self.emit(move |s| {
                    s.message = format!(
                        "Found {} apps locally ({} installed).",
                        report.apps_seen, report.installed
                    );
                }),
                Err(e) => self.emit(move |s| s.message = format!("Local scan failed: {e}")),
            }
        }

        let key = db.setting("steam_api_key").unwrap_or_default();
        let steamid = db.setting("steamid").unwrap_or_default();
        if !key.trim().is_empty() && !steamid.trim().is_empty() {
            self.emit(|s| {
                s.phase = "owned".into();
                s.message = "Asking Steam for the full owned list…".into();
            });
            match web::fetch_owned(http, key.trim(), steamid.trim()).await {
                Ok(games) => {
                    let n = web::ingest_owned(db, &games)?;
                    self.emit(move |s| s.message = format!("Steam says you own {n} games."));
                }
                Err(e) => {
                    self.emit(move |s| s.message = format!("Web API skipped: {e}"));
                }
            }
        }

        let size = db
            .with(|c| Ok(c.query_row("SELECT COUNT(*) FROM games", [], |r| r.get::<_, i64>(0))?))?;
        self.emit(move |s| s.library_size = size);

        let stale_days = if full { 0 } else { 45 };
        let pending = web::pending_enrichment(db, stale_days)?;
        let total = pending.len() as i64;
        self.emit(move |s| {
            s.phase = "enrich".into();
            s.total = total;
            s.done = 0;
            s.message = format!("Learning what {total} of your games actually are…");
        });

        let cc = db.setting_or("store_cc", "us");
        let lang = db.setting_or("store_lang", "en");
        let mut delay = Duration::from_millis(1400);
        let mut done = 0i64;
        let mut consecutive_limits = 0;

        for appid in pending {
            if self.cancel.load(Ordering::Relaxed) {
                self.emit(|s| s.message = "Stopped by you.".into());
                break;
            }
            match web::fetch_details(http, appid, &cc, &lang).await {
                Ok(web::FetchOutcome::Got(d)) => {
                    web::store_details(db, appid, &d)?;
                    consecutive_limits = 0;
                    delay = Duration::from_millis(1400);
                }
                Ok(web::FetchOutcome::NotAGame) => {
                    web::mark_unfetchable(db, appid)?;
                    consecutive_limits = 0;
                }
                Ok(web::FetchOutcome::RateLimited) => {
                    consecutive_limits += 1;
                    if consecutive_limits > 6 {
                        self.emit(|s| {
                            s.message =
                                "Steam is rate-limiting hard. Pausing enrichment — rerun later."
                                    .into()
                        });
                        break;
                    }
                    let backoff = Duration::from_secs(30 * consecutive_limits as u64);
                    self.emit(move |s| {
                        s.message = format!(
                            "Steam is rate-limiting. Waiting {}s and continuing…",
                            backoff.as_secs()
                        )
                    });
                    tokio::time::sleep(backoff).await;
                    delay = Duration::from_millis(2500);
                    continue;
                }
                Err(_) => {}
            }
            done += 1;
            let name = crate::engine::load::name_of(db, appid);
            self.emit(move |s| {
                s.done = done;
                s.enriched = done;
                s.message = format!("Learned {name}");
            });
            tokio::time::sleep(delay).await;
        }

        if !key.trim().is_empty()
            && !steamid.trim().is_empty()
            && !self.cancel.load(Ordering::Relaxed)
        {
            self.sync_achievements(db, http, key.trim(), steamid.trim())
                .await?;
        }

        self.emit(|s| {
            s.phase = "art".into();
            s.message = "Caching artwork so this works offline…".into();
        });
        let hot: Vec<i64> = db.with(|c| {
            let mut st = c.prepare(
                "SELECT appid FROM games ORDER BY installed DESC, playtime_min DESC LIMIT 120",
            )?;
            let rows: Vec<i64> = st
                .query_map([], |r| r.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })?;
        for appid in hot {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            images::prewarm(http, img_dir, appid).await;
        }

        db.set_setting("last_sync", &now().to_string())?;
        Ok(())
    }

    /// Achievement progress turns "you're 80% done" into a real, checkable claim.
    async fn sync_achievements(
        &self,
        db: &Db,
        http: &reqwest::Client,
        key: &str,
        steamid: &str,
    ) -> Result<()> {
        let targets: Vec<i64> = db.with(|c| {
            let mut st = c.prepare(
                "SELECT g.appid FROM games g
                 JOIN meta m ON m.appid=g.appid
                 LEFT JOIN achievements a ON a.appid=g.appid
                 WHERE g.playtime_min > 30 AND m.achievements_total > 0
                   AND (a.appid IS NULL OR a.fetched_at < ?1)
                 ORDER BY g.playtime_min DESC LIMIT 150",
            )?;
            let rows: Vec<i64> = st
                .query_map([now() - 7 * 86400], |r| r.get::<_, i64>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })?;

        let total = targets.len() as i64;
        if total == 0 {
            return Ok(());
        }
        self.emit(move |s| {
            s.phase = "achievements".into();
            s.total = total;
            s.done = 0;
            s.message = "Checking how close you are to finishing things…".into();
        });

        for (i, appid) in targets.into_iter().enumerate() {
            if self.cancel.load(Ordering::Relaxed) {
                break;
            }
            let url = format!(
                "https://api.steampowered.com/ISteamUserStats/GetPlayerAchievements/v1/?appid={appid}&key={key}&steamid={steamid}"
            );
            if let Ok(res) = http.get(url).send().await {
                if res.status().is_success() {
                    if let Ok(v) = res.json::<serde_json::Value>().await {
                        if let Some(arr) = v
                            .pointer("/playerstats/achievements")
                            .and_then(|a| a.as_array())
                        {
                            let total_a = arr.len() as i64;
                            let unlocked = arr
                                .iter()
                                .filter(|a| {
                                    a.get("achieved").and_then(serde_json::Value::as_i64) == Some(1)
                                })
                                .count() as i64;
                            db.with(|c| {
                                c.execute(
                                    "INSERT INTO achievements(appid,unlocked,total,fetched_at)
                                     VALUES(?1,?2,?3,?4)
                                     ON CONFLICT(appid) DO UPDATE SET
                                       unlocked=?2, total=?3, fetched_at=?4",
                                    rusqlite::params![appid, unlocked, total_a, now()],
                                )?;
                                Ok(())
                            })?;
                        }
                    }
                }
            }
            let done = i as i64 + 1;
            self.emit(move |s| s.done = done);
            tokio::time::sleep(Duration::from_millis(350)).await;
        }
        Ok(())
    }
}
