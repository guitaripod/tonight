use anyhow::Result;
use std::path::{Path, PathBuf};

pub const KINDS: &[&str] = &["capsule", "hero", "header", "logo"];

fn candidate_urls(appid: i64, kind: &str) -> Vec<String> {
    let file = match kind {
        "capsule" => "library_600x900.jpg",
        "hero" => "library_hero.jpg",
        "logo" => "logo.png",
        _ => "header.jpg",
    };
    vec![
        format!(
            "https://shared.cloudflare.steamstatic.com/store_item_assets/steam/apps/{appid}/{file}"
        ),
        format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{appid}/{file}"),
        format!("https://steamcdn-a.akamaihd.net/steam/apps/{appid}/{file}"),
    ]
}

fn cache_path(dir: &Path, appid: i64, kind: &str) -> PathBuf {
    let ext = if kind == "logo" { "png" } else { "jpg" };
    dir.join(format!("{appid}_{kind}.{ext}"))
}

fn miss_path(dir: &Path, appid: i64, kind: &str) -> PathBuf {
    dir.join(format!("{appid}_{kind}.miss"))
}

pub struct Image {
    pub bytes: Vec<u8>,
    pub content_type: &'static str,
}

/// Serves Steam artwork from a local disk cache, fetching once on first use.
/// The browser never talks to Valve; a fully-warmed instance works offline.
pub async fn get(
    http: &reqwest::Client,
    dir: &Path,
    appid: i64,
    kind: &str,
) -> Result<Option<Image>> {
    let kind = if KINDS.contains(&kind) {
        kind
    } else {
        "header"
    };
    let ct = if kind == "logo" {
        "image/png"
    } else {
        "image/jpeg"
    };
    let path = cache_path(dir, appid, kind);
    if let Ok(bytes) = tokio::fs::read(&path).await {
        if !bytes.is_empty() {
            return Ok(Some(Image {
                bytes,
                content_type: ct,
            }));
        }
    }
    let miss = miss_path(dir, appid, kind);
    if let Ok(meta) = tokio::fs::metadata(&miss).await {
        let age = meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if age < 7 * 86400 {
            return Ok(None);
        }
    }

    for url in candidate_urls(appid, kind) {
        let Ok(res) = http.get(&url).send().await else {
            continue;
        };
        if !res.status().is_success() {
            continue;
        }
        let Ok(bytes) = res.bytes().await else {
            continue;
        };
        if bytes.len() < 512 {
            continue;
        }
        let bytes = bytes.to_vec();
        let _ = tokio::fs::write(&path, &bytes).await;
        let _ = tokio::fs::remove_file(&miss).await;
        return Ok(Some(Image {
            bytes,
            content_type: ct,
        }));
    }
    let _ = tokio::fs::write(&miss, b"").await;
    Ok(None)
}

pub async fn prewarm(http: &reqwest::Client, dir: &Path, appid: i64) {
    for kind in ["capsule", "hero"] {
        let _ = get(http, dir, appid, kind).await;
    }
}

pub fn cached_count(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    let n = e.file_name();
                    let n = n.to_string_lossy();
                    n.ends_with(".jpg") || n.ends_with(".png")
                })
                .count()
        })
        .unwrap_or(0)
}
