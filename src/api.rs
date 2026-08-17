use crate::db::{now, Db};
use crate::engine::{deal, load, reckoning, session, shelves, taste};
use crate::model::{DealRequest, APPETITES, BUDGETS, MOODS};
use crate::steam::images;
use crate::sync::Syncer;
use crate::{launch, Config};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub http: reqwest::Client,
    pub cfg: Arc<Config>,
    pub syncer: Syncer,
}

pub struct ApiError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::warn!("api error: {:#}", self.0);
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

type ApiResult<T> = std::result::Result<T, ApiError>;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/state", get(state_handler))
        .route("/api/settings", post(save_settings))
        .route("/api/sync", post(start_sync))
        .route("/api/sync/cancel", post(cancel_sync))
        .route("/api/sync/stream", get(sync_stream))
        .route("/api/deal", post(deal_handler))
        .route("/api/hand/{id}", get(hand_handler))
        .route("/api/hand/{id}/pass", post(pass_handler))
        .route("/api/rebound", post(rebound_handler))
        .route("/api/session", get(session_handler))
        .route("/api/session/start", post(session_start))
        .route("/api/session/impression", post(session_impression))
        .route("/api/session/end", post(session_end))
        .route("/api/sessions", get(sessions_handler))
        .route("/api/launch", post(launch_handler))
        .route("/api/duel", get(duel_handler))
        .route("/api/duel", post(duel_result_handler))
        .route("/api/reckoning", get(reckoning_handler))
        .route("/api/amnesty/candidates", get(amnesty_candidates))
        .route("/api/amnesty", post(amnesty_handler))
        .route("/api/pin", post(pin_handler))
        .route("/api/game/{appid}", get(game_handler))
        .route("/api/game/{appid}/session-length", post(session_length))
        .route("/api/shelves", get(shelves_handler))
        .route("/api/shelf/{id}", get(shelf_handler))
        .route("/api/search", get(search_handler))
        .route("/api/taste", get(taste_handler))
        .route("/api/export", get(export_handler))
        .route("/img/{appid}/{kind}", get(image_handler))
        .with_state(state)
}

async fn state_handler(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    let db = &st.db;
    let counts: (i64, i64, i64, i64, i64) = db.with(|c| {
        Ok(c.query_row(
            "SELECT (SELECT COUNT(*) FROM games),
                    (SELECT COUNT(*) FROM meta WHERE ok=1 AND fetched_at>0),
                    (SELECT COUNT(*) FROM games WHERE installed=1),
                    (SELECT COUNT(*) FROM prefs WHERE amnesty=1),
                    (SELECT COUNT(*) FROM sessions)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?)
    })?;
    let has_key = !db.setting_or("steam_api_key", "").trim().is_empty();
    Ok(Json(json!({
        "library": { "games": counts.0, "enriched": counts.1, "installed": counts.2,
                     "retired": counts.3, "sessions": counts.4 },
        "steam_root": st.cfg.steam_root.as_ref().map(|p| p.display().to_string()),
        "data_dir": st.cfg.data_dir.display().to_string(),
        "settings": {
            "steamid": db.setting_or("steamid", ""),
            "has_api_key": has_key,
            "allow_launch": db.setting_bool("allow_launch", false),
            "first_impression_minutes": db.setting_i64("first_impression_minutes", 12),
            "hours_per_week": db.setting_or("hours_per_week", ""),
            "store_cc": db.setting_or("store_cc", "us"),
            "reroll_friction": db.setting_bool("reroll_friction", true),
        },
        "launch_possible": launch::available(),
        "last_sync": db.setting_i64("last_sync", 0),
        "sync": st.syncer.status(),
        "artwork_cached": images::cached_count(&st.cfg.img_dir()),
        "moods": MOODS.iter().map(|(id,label,hint)| json!({"id":id,"label":label,"hint":hint})).collect::<Vec<_>>(),
        "appetites": APPETITES.iter().map(|(id,label,hint)| json!({"id":id,"label":label,"hint":hint})).collect::<Vec<_>>(),
        "budgets": BUDGETS.iter().map(|(m,label)| json!({"minutes":m,"label":label})).collect::<Vec<_>>(),
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

#[derive(Deserialize)]
struct SettingsBody {
    #[serde(default)]
    steam_api_key: Option<String>,
    #[serde(default)]
    steamid: Option<String>,
    #[serde(default)]
    allow_launch: Option<bool>,
    #[serde(default)]
    first_impression_minutes: Option<i64>,
    #[serde(default)]
    hours_per_week: Option<String>,
    #[serde(default)]
    store_cc: Option<String>,
    #[serde(default)]
    reroll_friction: Option<bool>,
}

async fn save_settings(
    State(st): State<AppState>,
    Json(body): Json<SettingsBody>,
) -> ApiResult<Json<Value>> {
    if let Some(v) = body.steam_api_key {
        st.db.set_setting("steam_api_key", v.trim())?;
    }
    if let Some(v) = body.steamid {
        st.db.set_setting("steamid", v.trim())?;
    }
    if let Some(v) = body.allow_launch {
        st.db
            .set_setting("allow_launch", if v { "1" } else { "0" })?;
    }
    if let Some(v) = body.first_impression_minutes {
        st.db
            .set_setting("first_impression_minutes", &v.clamp(1, 120).to_string())?;
    }
    if let Some(v) = body.hours_per_week {
        st.db.set_setting("hours_per_week", v.trim())?;
    }
    if let Some(v) = body.store_cc {
        st.db.set_setting("store_cc", v.trim())?;
    }
    if let Some(v) = body.reroll_friction {
        st.db
            .set_setting("reroll_friction", if v { "1" } else { "0" })?;
    }
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize, Default)]
struct SyncBody {
    #[serde(default)]
    full: bool,
}

async fn start_sync(
    State(st): State<AppState>,
    body: Option<Json<SyncBody>>,
) -> ApiResult<Json<Value>> {
    let full = body.map(|b| b.0.full).unwrap_or(false);
    let started = st.syncer.spawn(
        st.db.clone(),
        st.http.clone(),
        st.cfg.steam_root.clone(),
        st.cfg.img_dir(),
        full,
    );
    Ok(Json(
        json!({"started": started, "status": st.syncer.status()}),
    ))
}

async fn cancel_sync(State(st): State<AppState>) -> Json<Value> {
    st.syncer.cancel();
    Json(json!({"ok": true}))
}

async fn sync_stream(
    State(st): State<AppState>,
) -> Sse<
    impl futures::Stream<
        Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>,
    >,
> {
    let stream = async_stream::stream(st.syncer.rx.clone());
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
}

mod async_stream {
    use axum::response::sse::Event;
    use futures::Stream;
    use tokio::sync::watch;

    /// Turns the sync status watch channel into an SSE stream that always emits
    /// the current value first, so a late-joining tab is never blank.
    pub fn stream(
        rx: watch::Receiver<crate::sync::SyncStatus>,
    ) -> impl Stream<Item = std::result::Result<Event, std::convert::Infallible>> {
        futures::stream::unfold((rx, true), move |(mut rx, first)| async move {
            if !first && rx.changed().await.is_err() {
                return None;
            }
            let status = rx.borrow_and_update().clone();
            let ev = Event::default().json_data(&status).unwrap_or_default();
            Some((Ok(ev), (rx, false)))
        })
    }
}

async fn deal_handler(
    State(st): State<AppState>,
    Json(req): Json<DealRequest>,
) -> ApiResult<Json<Value>> {
    let hand = deal::deal(&st.db, &req)?;
    Ok(Json(serde_json::to_value(hand)?))
}

async fn hand_handler(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(deal::load_hand(&st.db, &id)?)?))
}

#[derive(Deserialize)]
struct PassBody {
    appid: i64,
}

async fn pass_handler(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PassBody>,
) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(deal::pass(
        &st.db, &id, body.appid,
    )?)?))
}

#[derive(Deserialize)]
struct ReboundBody {
    #[serde(default)]
    budget: i64,
    #[serde(default)]
    mood: String,
    #[serde(default)]
    appetite: String,
    #[serde(default)]
    exclude: Vec<i64>,
}

async fn rebound_handler(
    State(st): State<AppState>,
    Json(b): Json<ReboundBody>,
) -> ApiResult<Json<Value>> {
    let card = deal::rebound(&st.db, b.budget, &b.mood, &b.appetite, b.exclude)?;
    Ok(Json(serde_json::to_value(card)?))
}

async fn session_handler(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "session": session::current(&st.db)? })))
}

#[derive(Deserialize)]
struct StartBody {
    appid: i64,
    #[serde(default)]
    hand_id: Option<String>,
    #[serde(default)]
    budget: i64,
    #[serde(default)]
    mood: String,
    #[serde(default)]
    appetite: String,
}

async fn session_start(
    State(st): State<AppState>,
    Json(b): Json<StartBody>,
) -> ApiResult<Json<Value>> {
    let s = session::start(
        &st.db,
        b.appid,
        b.hand_id.as_deref(),
        b.budget,
        &b.mood,
        &b.appetite,
    )?;
    Ok(Json(json!({ "session": s })))
}

#[derive(Deserialize)]
struct ImpressionBody {
    id: i64,
    answer: String,
}

async fn session_impression(
    State(st): State<AppState>,
    Json(b): Json<ImpressionBody>,
) -> ApiResult<Json<Value>> {
    session::first_impression(&st.db, b.id, &b.answer)?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct EndBody {
    id: i64,
    verdict: String,
}

async fn session_end(State(st): State<AppState>, Json(b): Json<EndBody>) -> ApiResult<Json<Value>> {
    let appid = session::end(&st.db, b.id, &b.verdict)?;
    Ok(Json(json!({"ok": true, "appid": appid})))
}

async fn sessions_handler(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "sessions": session::recent(&st.db, 40)? })))
}

#[derive(Deserialize)]
struct LaunchBody {
    appid: i64,
}

async fn launch_handler(
    State(st): State<AppState>,
    Json(b): Json<LaunchBody>,
) -> ApiResult<Json<Value>> {
    if !st.db.setting_bool("allow_launch", false) {
        return Err(ApiError(anyhow::anyhow!(
            "Launching from the server is switched off. Turn it on in Settings, or use the steam:// link."
        )));
    }
    let msg = launch::launch(b.appid).await?;
    st.db
        .log_event(b.appid, "launched", None, None, None, None)?;
    Ok(Json(json!({"ok": true, "detail": msg})))
}

#[derive(Deserialize)]
struct DuelQuery {
    #[serde(default)]
    exclude: String,
    #[serde(default = "one")]
    round: i64,
    #[serde(default = "seven")]
    of: i64,
}

fn one() -> i64 {
    1
}
fn seven() -> i64 {
    7
}

async fn duel_handler(
    State(st): State<AppState>,
    Query(q): Query<DuelQuery>,
) -> ApiResult<Json<Value>> {
    let exclude: Vec<i64> = q
        .exclude
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    Ok(Json(serde_json::to_value(taste::duel(
        &st.db, &exclude, q.round, q.of,
    )?)?))
}

#[derive(Deserialize)]
struct DuelBody {
    winner: i64,
    loser: i64,
}

async fn duel_result_handler(
    State(st): State<AppState>,
    Json(b): Json<DuelBody>,
) -> ApiResult<Json<Value>> {
    taste::duel_result(&st.db, b.winner, b.loser)?;
    Ok(Json(json!({"ok": true})))
}

async fn reckoning_handler(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(serde_json::to_value(reckoning::compute(&st.db)?)?))
}

async fn amnesty_candidates(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({
        "candidates": reckoning::amnesty_candidates(&st.db, 12)?
    })))
}

#[derive(Deserialize)]
struct AmnestyBody {
    appid: i64,
    #[serde(default = "yes")]
    on: bool,
}

fn yes() -> bool {
    true
}

async fn amnesty_handler(
    State(st): State<AppState>,
    Json(b): Json<AmnestyBody>,
) -> ApiResult<Json<Value>> {
    reckoning::set_amnesty(&st.db, b.appid, b.on)?;
    Ok(Json(json!({"ok": true})))
}

async fn pin_handler(
    State(st): State<AppState>,
    Json(b): Json<AmnestyBody>,
) -> ApiResult<Json<Value>> {
    reckoning::set_pinned(&st.db, b.appid, b.on)?;
    Ok(Json(json!({"ok": true})))
}

async fn game_handler(
    State(st): State<AppState>,
    Path(appid): Path<i64>,
) -> ApiResult<Json<Value>> {
    let c = load::one(&st.db, appid)?
        .ok_or_else(|| ApiError(anyhow::anyhow!("no such game in your library")))?;
    Ok(Json(json!({
        "game": c.game,
        "meta": c.meta,
        "shape": c.shape,
        "history": c.history,
        "amnesty": c.prefs.amnesty,
        "pinned": c.prefs.pinned,
        "elo": c.prefs.elo,
        "session_override": c.prefs.session_override,
        "store_url": format!("https://store.steampowered.com/app/{appid}"),
        "launch_url": format!("steam://rungameid/{appid}"),
    })))
}

#[derive(Deserialize)]
struct LengthBody {
    minutes: Option<i64>,
}

async fn session_length(
    State(st): State<AppState>,
    Path(appid): Path<i64>,
    Json(b): Json<LengthBody>,
) -> ApiResult<Json<Value>> {
    st.db.with(|c| {
        c.execute(
            "INSERT INTO prefs(appid,session_minutes,updated_at) VALUES(?1,?2,?3)
             ON CONFLICT(appid) DO UPDATE SET session_minutes=?2, updated_at=?3",
            rusqlite::params![appid, b.minutes, now()],
        )?;
        Ok(())
    })?;
    Ok(Json(json!({"ok": true})))
}

async fn shelves_handler() -> Json<Value> {
    Json(json!({ "shelves": shelves::SHELVES }))
}

#[derive(Deserialize)]
struct LimitQuery {
    #[serde(default = "fifty")]
    limit: usize,
}

fn fifty() -> usize {
    50
}

async fn shelf_handler(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<LimitQuery>,
) -> ApiResult<Json<Value>> {
    let items = shelves::shelf(&st.db, &id, q.limit.min(400))?;
    let def = shelves::SHELVES.iter().find(|s| s.id == id);
    Ok(Json(json!({
        "id": id,
        "title": def.map(|d| d.title),
        "question": def.map(|d| d.question),
        "count": items.len(),
        "items": items,
    })))
}

#[derive(Deserialize)]
struct SearchQuery {
    #[serde(default)]
    q: String,
}

async fn search_handler(
    State(st): State<AppState>,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "items": shelves::search(&st.db, &q.q, 40)? })))
}

async fn taste_handler(State(st): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "taste": taste::profile(&st.db)? })))
}

#[derive(Serialize)]
struct Export {
    version: u32,
    exported_at: i64,
    settings: Vec<(String, String)>,
    prefs: Vec<Value>,
    events: Vec<Value>,
    sessions: Vec<Value>,
    taste: Vec<Value>,
}

/// Your judgements are yours: one file, no lock-in, restorable into a fresh install.
async fn export_handler(State(st): State<AppState>) -> ApiResult<Response> {
    let db = &st.db;
    let dump = |sql: &str| -> anyhow::Result<Vec<Value>> {
        db.with(|c| {
            let mut stmt = c.prepare(sql)?;
            let cols: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
            let rows: Vec<Value> = stmt
                .query_map([], |r| {
                    let mut m = serde_json::Map::new();
                    for (i, name) in cols.iter().enumerate() {
                        let v: rusqlite::types::Value = r.get(i)?;
                        m.insert(
                            name.clone(),
                            match v {
                                rusqlite::types::Value::Null => Value::Null,
                                rusqlite::types::Value::Integer(i) => json!(i),
                                rusqlite::types::Value::Real(f) => json!(f),
                                rusqlite::types::Value::Text(t) => json!(t),
                                rusqlite::types::Value::Blob(_) => Value::Null,
                            },
                        );
                    }
                    Ok(Value::Object(m))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
    };
    let settings = db.with(|c| {
        let mut s = c.prepare("SELECT k,v FROM settings WHERE k <> 'steam_api_key'")?;
        let rows: Vec<(String, String)> = s
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    let export = Export {
        version: 1,
        exported_at: now(),
        settings,
        prefs: dump("SELECT * FROM prefs")?,
        events: dump("SELECT * FROM events")?,
        sessions: dump("SELECT * FROM sessions")?,
        taste: dump("SELECT * FROM taste")?,
    };
    let body = serde_json::to_vec_pretty(&export)?;
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"tonight-export.json\""
            .parse()
            .unwrap(),
    );
    Ok((headers, body).into_response())
}

async fn image_handler(
    State(st): State<AppState>,
    Path((appid, kind)): Path<(i64, String)>,
) -> Response {
    match images::get(&st.http, &st.cfg.img_dir(), appid, &kind).await {
        Ok(Some(img)) => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, img.content_type.parse().unwrap());
            headers.insert(
                header::CACHE_CONTROL,
                "public, max-age=604800, immutable".parse().unwrap(),
            );
            (headers, img.bytes).into_response()
        }
        _ => (StatusCode::NOT_FOUND, "no artwork").into_response(),
    }
}
