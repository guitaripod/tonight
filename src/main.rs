mod api;
mod config;
mod db;
mod engine;
mod launch;
mod model;
mod steam;
mod sync;

pub use config::Config;

use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;

#[derive(Embed)]
#[folder = "web/"]
struct Assets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TONIGHT_LOG")
                .unwrap_or_else(|_| "tonight=info,tower_http=warn".into()),
        )
        .with_target(false)
        .compact()
        .init();

    let cfg = Arc::new(Config::from_env_and_args()?);
    let db = db::Db::open(&cfg.db_path())?;
    let http = steam::web::client();
    let syncer = sync::Syncer::new();

    if let Some(root) = &cfg.steam_root {
        match steam::local::scan_and_ingest(&db, root) {
            Ok(report) => tracing::info!(
                "read {} apps from {} ({} installed)",
                report.apps_seen,
                report.steam_root,
                report.installed
            ),
            Err(e) => tracing::warn!("local scan failed: {e:#}"),
        }
        watch_steam(db.clone(), root.clone());
    }

    let state = api::AppState {
        db: db.clone(),
        http: http.clone(),
        cfg: cfg.clone(),
        syncer: syncer.clone(),
    };

    let app = Router::new()
        .merge(api::router(state))
        .route("/", get(index))
        .route("/{*path}", get(static_handler))
        .layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind(cfg.addr).await?;
    let local = listener.local_addr()?;
    let games: i64 = db
        .with(|c| Ok(c.query_row("SELECT COUNT(*) FROM games", [], |r| r.get(0))?))
        .unwrap_or(0);

    println!(
        "\n  tonight {v}\n  → http://{addr}\n  {games} games in the library · data in {data}\n{steam}\n  Stop browsing. Deal a hand.\n",
        v = env!("CARGO_PKG_VERSION"),
        addr = friendly(&local),
        data = cfg.data_dir.display(),
        steam = match &cfg.steam_root {
            Some(p) => format!("  Steam found at {}", p.display()),
            None => "  No local Steam install found — add an API key in Settings".into(),
        }
    );

    if cfg.open_browser {
        let url = format!("http://{}", friendly(&local));
        tokio::spawn(async move {
            let _ = open_in_browser(&url);
        });
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

/// Rereads the Steam client every couple of minutes, so a game installed or
/// played while tonight is running shows up without pressing sync.
fn watch_steam(db: db::Db, root: std::path::PathBuf) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(120)).await;
            let (db, root) = (db.clone(), root.clone());
            let _ = tokio::task::spawn_blocking(move || steam::local::scan_and_ingest(&db, &root))
                .await;
        }
    });
}

fn open_in_browser(url: &str) -> std::io::Result<()> {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = tokio::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = tokio::process::Command::new("open");
        c.arg(url);
        c
    } else {
        let mut c = tokio::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    cmd.spawn().map(|_| ())
}

fn friendly(addr: &std::net::SocketAddr) -> String {
    if addr.ip().is_unspecified() {
        format!("localhost:{}", addr.port())
    } else {
        addr.to_string()
    }
}

/// Resolves on Ctrl-C everywhere, and on SIGTERM where it exists, so that
/// `docker stop` and systemd end the server cleanly instead of timing out.
async fn shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            },
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    println!("\n  Goodnight.\n");
}

async fn index() -> Response {
    serve("index.html")
}

async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return serve("index.html");
    }
    if Assets::get(path).is_none() && !path.contains('.') {
        return serve("index.html");
    }
    serve(path)
}

fn serve(path: &str) -> Response {
    match Assets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, mime.as_ref().parse().unwrap());
            headers.insert(header::CACHE_CONTROL, "no-cache".parse().unwrap());
            (headers, file.data.into_owned()).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
