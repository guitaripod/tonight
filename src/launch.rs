use anyhow::{anyhow, Result};
use tokio::process::Command;

/// Launches a game on the machine running tonight, so a phone on the sofa can
/// start the PC. Off by default — it is the one thing here that touches the OS.
pub async fn launch(appid: i64) -> Result<String> {
    let url = format!("steam://rungameid/{appid}");

    if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()?;
        return Ok(format!("Asked Windows to open {url}"));
    }
    if cfg!(target_os = "macos") {
        Command::new("open").arg(&url).spawn()?;
        return Ok(format!("Asked macOS to open {url}"));
    }

    if which("steam") {
        Command::new("steam")
            .arg("-applaunch")
            .arg(appid.to_string())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        return Ok(format!("steam -applaunch {appid}"));
    }
    if which("flatpak") && flatpak_has_steam().await {
        Command::new("flatpak")
            .args([
                "run",
                "com.valvesoftware.Steam",
                "-applaunch",
                &appid.to_string(),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        return Ok(format!("flatpak Steam -applaunch {appid}"));
    }
    if which("xdg-open") {
        Command::new("xdg-open").arg(&url).spawn()?;
        return Ok(format!("xdg-open {url}"));
    }
    Err(anyhow!(
        "No way to launch on this host — no steam, flatpak or xdg-open on PATH"
    ))
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|p| {
                let f = p.join(bin);
                f.is_file()
            })
        })
        .unwrap_or(false)
}

async fn flatpak_has_steam() -> bool {
    Command::new("flatpak")
        .args(["info", "com.valvesoftware.Steam"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn available() -> bool {
    cfg!(target_os = "windows")
        || cfg!(target_os = "macos")
        || which("steam")
        || which("flatpak")
        || which("xdg-open")
}
